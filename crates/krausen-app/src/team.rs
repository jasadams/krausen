// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared, workspace-scoped membership operations.
use crate::{ActorContext, Error, authorize_workspace, boundary_error, db_error};
use krausen_core::{DbPool, DbTx};
use krausen_types::account::{TeamMember, TeamSnapshot};

#[derive(sqlx::FromRow)]
struct MemberRow {
    user_id: String,
    email: String,
    name: Option<String>,
    role: String,
    owner_user_id: String,
    joined_at: String,
}

pub fn project_change(result: Result<i64, Error>) -> krausen_types::account::TeamChangeOutcome {
    use krausen_types::account::TeamChangeOutcome as Outcome;
    match result {
        Ok(revision) => Outcome::Saved { revision },
        Err(Error::Conflict { expected, actual }) => Outcome::Conflict { expected, actual },
        Err(Error::RequestIdentityConflict) => Outcome::RequestIdentityConflict,
        Err(Error::ResultExpired { .. }) => Outcome::ResultExpired,
        Err(Error::Forbidden) => Outcome::Forbidden,
        Err(Error::Invalid(_)) => Outcome::Invalid,
        Err(Error::Retryable) => Outcome::Retryable,
        Err(error) => {
            tracing::error!(%error, "team change failed");
            Outcome::Unavailable
        }
    }
}

pub fn project_invite(
    result: Result<krausen_types::account::InvitationCreated, Error>,
) -> krausen_types::account::InvitationCreateOutcome {
    use krausen_types::account::InvitationCreateOutcome as Outcome;
    match result {
        Ok(created) => Outcome::Queued(created),
        Err(Error::Conflict { expected, actual }) => Outcome::Conflict { expected, actual },
        Err(Error::RequestIdentityConflict) => Outcome::RequestIdentityConflict,
        Err(Error::ResultExpired { .. }) => Outcome::ResultExpired,
        Err(Error::Forbidden) => Outcome::Forbidden,
        Err(Error::Invalid(_)) => Outcome::Invalid,
        Err(Error::Retryable) => Outcome::Retryable,
        Err(Error::DeliveryUnavailable) => Outcome::DeliveryUnavailable,
        Err(error) => {
            tracing::error!(%error, "invitation creation failed");
            Outcome::Unavailable
        }
    }
}

/// The member list and resource revision are read in one transaction under current authority.
pub async fn members(pool: &DbPool, actor: &ActorContext) -> Result<TeamSnapshot, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    let workspace_sql = if matches!(tx, DbTx::Postgres(_)) {
        "SELECT revision FROM workspaces WHERE workspace_id=$1 AND status='active' FOR SHARE"
    } else {
        "SELECT revision FROM workspaces WHERE workspace_id=$1 AND status='active'"
    };
    let revision = scalar!(&mut tx, i64, workspace_sql, &actor.workspace_id)
        .map_err(db_error)?
        .ok_or(Error::Forbidden)?;
    let sql = "SELECT u.user_id, u.email, u.name, m.role, w.owner_user_id, CAST(m.created_at AS TEXT) AS joined_at FROM workspace_users m JOIN users u ON u.user_id=m.user_id JOIN workspaces w ON w.workspace_id=m.workspace_id WHERE m.workspace_id=$1 AND m.active=TRUE ORDER BY u.user_id";
    let rows = match &mut tx {
        DbTx::Postgres(t) => {
            sqlx::query_as::<_, MemberRow>(sql)
                .bind(&actor.workspace_id)
                .fetch_all(&mut **t)
                .await
        }
        DbTx::Sqlite(t) => {
            sqlx::query_as::<_, MemberRow>(sql)
                .bind(&actor.workspace_id)
                .fetch_all(&mut **t)
                .await
        }
    }
    .map_err(db_error)?;
    authorize_workspace(&mut tx, actor).await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(TeamSnapshot {
        revision,
        members: rows
            .into_iter()
            .map(|row| TeamMember {
                is_owner: row.user_id == row.owner_user_id,
                user_id: row.user_id,
                email: row.email,
                name: row.name,
                role: row.role,
                joined_at: row.joined_at,
            })
            .collect(),
    })
}

/// Admin membership mutation; serializes with workspace edits and authority changes.
pub async fn change(
    pool: &DbPool,
    actor: &ActorContext,
    change: krausen_types::account::TeamChange,
    expected: i64,
    request_id: uuid::Uuid,
) -> Result<i64, Error> {
    use krausen_types::account::TeamChange;
    let (user_id, next_role) = match &change {
        TeamChange::SetRole { user_id, role }
            if role == "workspace_admin" || role == "workspace_user" =>
        {
            (user_id, Some(role.as_str()))
        }
        TeamChange::SetRole { .. } => return Err(Error::Invalid("member role")),
        TeamChange::Remove { user_id } => (user_id, None),
    };
    let digest = krausen_core::canonical_json::sha256(
        &serde_json::json!({"change":change,"expected_revision":expected}),
    )
    .map_err(|_| Error::Invalid("member change identity"))?;
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    if authorize_workspace(&mut tx, actor).await? != "workspace_admin" {
        return Err(Error::Forbidden);
    }
    let pg = matches!(tx, DbTx::Postgres(_));
    let workspace_sql = if pg {
        "SELECT revision FROM workspaces WHERE workspace_id=$1 AND status='active' FOR UPDATE"
    } else {
        "SELECT revision FROM workspaces WHERE workspace_id=$1 AND status='active'"
    };
    let revision = scalar!(&mut tx, i64, workspace_sql, &actor.workspace_id)
        .map_err(db_error)?
        .ok_or(Error::Forbidden)?;
    if authorize_workspace(&mut tx, actor).await? != "workspace_admin" {
        return Err(Error::Forbidden);
    }
    let id = request_id.to_string();
    let key = crate::RequestIdentity {
        actor,
        operation: "workspace.member.change.v1",
        request_id: &id,
        payload_digest: &digest,
    };
    if let Some(result) = crate::recover_request(&mut tx, &key).await? {
        let revision = serde_json::from_value(result)?;
        tx.commit().await.map_err(boundary_error)?;
        return Ok(revision);
    }
    if revision != expected {
        return Err(Error::Conflict {
            expected,
            actual: revision,
        });
    }
    let owner = scalar!(
        &mut tx,
        String,
        "SELECT owner_user_id FROM workspaces WHERE workspace_id=$1",
        &actor.workspace_id
    )
    .map_err(db_error)?
    .ok_or(Error::Forbidden)?;
    if owner == *user_id {
        return Err(Error::Invalid("workspace owner"));
    }
    let member_sql = if pg {
        "SELECT role FROM workspace_users WHERE workspace_id=$1 AND user_id=$2 AND active=TRUE FOR UPDATE"
    } else {
        "SELECT role FROM workspace_users WHERE workspace_id=$1 AND user_id=$2 AND active=TRUE"
    };
    let current = scalar!(&mut tx, String, member_sql, &actor.workspace_id, user_id)
        .map_err(db_error)?
        .ok_or(Error::Forbidden)?;
    if current == "workspace_admin" && next_role != Some("workspace_admin") {
        let count = scalar!(&mut tx, i64, "SELECT COUNT(*) FROM workspace_users WHERE workspace_id=$1 AND active=TRUE AND role='workspace_admin'", &actor.workspace_id).map_err(db_error)?.unwrap_or_default();
        if count <= 1 {
            return Err(Error::Invalid("last administrator"));
        }
    }
    match next_role {
        Some(role) => {
            query!(&mut tx, "UPDATE workspace_users SET role=$1 WHERE workspace_id=$2 AND user_id=$3 AND active=TRUE", role, &actor.workspace_id, user_id).map_err(db_error)?;
        }
        None => {
            query!(&mut tx, "UPDATE workspace_users SET active=FALSE WHERE workspace_id=$1 AND user_id=$2 AND active=TRUE", &actor.workspace_id, user_id).map_err(db_error)?;
        }
    }
    let next = revision + 1;
    query!(
        &mut tx,
        "UPDATE workspaces SET revision=$1 WHERE workspace_id=$2 AND revision=$3",
        next,
        &actor.workspace_id,
        revision
    )
    .map_err(db_error)?;
    crate::record_request(&mut tx, &key, &serde_json::json!(next), Some(user_id)).await?;
    crate::record_change(&mut tx, actor, "workspace.member.change.v1", user_id, next).await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(next)
}

/// Create an email-addressed invitation and durable notification together.
pub async fn invite(
    pool: &DbPool,
    actor: &ActorContext,
    email: &str,
    role: &str,
    expected: i64,
    request_id: uuid::Uuid,
    mail_available: bool,
) -> Result<krausen_types::account::InvitationCreated, Error> {
    let email = email.trim().to_lowercase();
    if email.len() > 254 || !email.contains('@') || email.chars().any(char::is_whitespace) {
        return Err(Error::Invalid("email"));
    }
    if !matches!(role, "workspace_admin" | "workspace_user") {
        return Err(Error::Invalid("member role"));
    }
    let digest = krausen_core::canonical_json::sha256(
        &serde_json::json!({"email":email,"role":role,"expected_revision":expected}),
    )
    .map_err(|_| Error::Invalid("invitation identity"))?;
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    if authorize_workspace(&mut tx, actor).await? != "workspace_admin" {
        return Err(Error::Forbidden);
    }
    let sql = if matches!(tx, DbTx::Postgres(_)) {
        "SELECT revision FROM workspaces WHERE workspace_id=$1 AND status='active' FOR UPDATE"
    } else {
        "SELECT revision FROM workspaces WHERE workspace_id=$1 AND status='active'"
    };
    let revision = scalar!(&mut tx, i64, sql, &actor.workspace_id)
        .map_err(db_error)?
        .ok_or(Error::Forbidden)?;
    if authorize_workspace(&mut tx, actor).await? != "workspace_admin" {
        return Err(Error::Forbidden);
    }
    let id = request_id.to_string();
    let key = crate::RequestIdentity {
        actor,
        operation: "workspace.invitation.create.v1",
        request_id: &id,
        payload_digest: &digest,
    };
    if let Some(result) = crate::recover_request(&mut tx, &key).await? {
        let result = serde_json::from_value(result)?;
        tx.commit().await.map_err(boundary_error)?;
        return Ok(result);
    }
    if !mail_available {
        return Err(Error::DeliveryUnavailable);
    }
    if revision != expected {
        return Err(Error::Conflict {
            expected,
            actual: revision,
        });
    }
    let duplicate = scalar!(&mut tx, i64, "SELECT COUNT(*) FROM workspace_users m JOIN users u ON u.user_id=m.user_id WHERE m.workspace_id=$1 AND m.active=TRUE AND lower(u.email)=lower($2)", &actor.workspace_id, &email).map_err(db_error)?.unwrap_or_default();
    let pending_sql = if matches!(tx, DbTx::Postgres(_)) {
        "SELECT COUNT(*) FROM workspace_invitations WHERE workspace_id=$1 AND lower(email)=lower($2) AND status='pending' AND expires_at>CURRENT_TIMESTAMP"
    } else {
        "SELECT COUNT(*) FROM workspace_invitations WHERE workspace_id=$1 AND lower(email)=lower($2) AND status='pending' AND julianday(expires_at)>julianday('now')"
    };
    let pending = scalar!(&mut tx, i64, pending_sql, &actor.workspace_id, &email)
        .map_err(db_error)?
        .unwrap_or_default();
    if duplicate != 0 || pending != 0 {
        return Err(Error::Invalid("invitation unavailable"));
    }
    let limit = scalar!(
        &mut tx,
        Option<i64>,
        "SELECT CAST(user_limit AS BIGINT) FROM workspaces WHERE workspace_id=$1",
        &actor.workspace_id
    )
    .map_err(db_error)?
    .flatten();
    if let Some(limit) = limit {
        let members = scalar!(
            &mut tx,
            i64,
            "SELECT COUNT(*) FROM workspace_users WHERE workspace_id=$1 AND active=TRUE",
            &actor.workspace_id
        )
        .map_err(db_error)?
        .unwrap_or_default();
        let pending_sql = if matches!(tx, DbTx::Postgres(_)) {
            "SELECT COUNT(*) FROM workspace_invitations WHERE workspace_id=$1 AND status='pending' AND expires_at>CURRENT_TIMESTAMP"
        } else {
            "SELECT COUNT(*) FROM workspace_invitations WHERE workspace_id=$1 AND status='pending' AND julianday(expires_at)>julianday('now')"
        };
        let pending = scalar!(&mut tx, i64, pending_sql, &actor.workspace_id)
            .map_err(db_error)?
            .unwrap_or_default();
        if members + pending >= limit {
            return Err(Error::Invalid("workspace member limit"));
        }
    }
    let invitation_id = uuid::Uuid::new_v4().to_string();
    let expires = chrono::Utc::now() + chrono::Duration::days(7);
    query!(&mut tx, "INSERT INTO workspace_invitations (invitation_id,workspace_id,email,role,invited_by_user_id,expires_at) VALUES ($1,$2,$3,$4,$5,$6)", &invitation_id, &actor.workspace_id, &email, role, &actor.actor_id, expires).map_err(db_error)?;
    let next = revision + 1;
    query!(
        &mut tx,
        "UPDATE workspaces SET revision=$1 WHERE workspace_id=$2 AND revision=$3",
        next,
        &actor.workspace_id,
        revision
    )
    .map_err(db_error)?;
    crate::enqueue_outbox(
        &mut tx,
        &actor.workspace_id,
        "workspace.invitation.notify.v1",
        &serde_json::json!({"invitation_id":invitation_id}),
    )
    .await?;
    let result = krausen_types::account::InvitationCreated {
        invitation_id: invitation_id.clone(),
        revision: next,
        delivery: "pending".into(),
    };
    crate::record_request(
        &mut tx,
        &key,
        &serde_json::to_value(&result)?,
        Some(&invitation_id),
    )
    .await?;
    crate::record_change(
        &mut tx,
        actor,
        "workspace.invitation.create.v1",
        &invitation_id,
        next,
    )
    .await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(result)
}

#[derive(sqlx::FromRow)]
struct InvitationRow {
    invitation_id: String,
    email: String,
    role: String,
    status: String,
    created_at: String,
    expires_at: String,
}

/// List pending invitations to the current workspace for a current administrator.
pub async fn invitations(
    pool: &DbPool,
    actor: &ActorContext,
) -> Result<krausen_types::account::BoundedList<krausen_types::account::TeamInvitation>, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    if authorize_workspace(&mut tx, actor).await? != "workspace_admin" {
        return Err(Error::Forbidden);
    }
    let workspace_sql = if matches!(tx, DbTx::Postgres(_)) {
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
    let sql = if matches!(tx, DbTx::Postgres(_)) {
        "SELECT invitation_id,email,role,status,CAST(created_at AS TEXT) AS created_at,CAST(expires_at AS TEXT) AS expires_at FROM workspace_invitations WHERE workspace_id=$1 AND status='pending' AND expires_at>CURRENT_TIMESTAMP ORDER BY invitation_id LIMIT 101"
    } else {
        "SELECT invitation_id,email,role,status,CAST(created_at AS TEXT) AS created_at,CAST(expires_at AS TEXT) AS expires_at FROM workspace_invitations WHERE workspace_id=$1 AND status='pending' AND julianday(expires_at)>julianday('now') ORDER BY invitation_id LIMIT 101"
    };
    let rows = match &mut tx {
        DbTx::Postgres(t) => {
            sqlx::query_as::<_, InvitationRow>(sql)
                .bind(&actor.workspace_id)
                .fetch_all(&mut **t)
                .await
        }
        DbTx::Sqlite(t) => {
            sqlx::query_as::<_, InvitationRow>(sql)
                .bind(&actor.workspace_id)
                .fetch_all(&mut **t)
                .await
        }
    }
    .map_err(db_error)?;
    authorize_workspace(&mut tx, actor).await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(crate::bounded(
        rows.into_iter()
            .map(|row| krausen_types::account::TeamInvitation {
                invitation_id: row.invitation_id,
                email: row.email,
                role: row.role,
                status: row.status,
                created_at: row.created_at,
                expires_at: row.expires_at,
            })
            .collect(),
    ))
}
