// SPDX-License-Identifier: AGPL-3.0-or-later
//! Invitation replies by an authenticated recipient, independent of membership.
use crate::{ActorContext, Error, RequestIdentity, boundary_error, db_error};
use krausen_core::{DbPool, DbTx};
use krausen_types::invitation::{InvitationAction, InvitationResponseOutcome as Outcome};

/// Identity established by the adapter; no caller-selected workspace or email.
pub struct AccountActor {
    pub actor_id: String,
    pub credential_id: String,
    pub access_expires_at: i64,
}

/// Lock and recheck account and credential authority without requiring a workspace.
pub(crate) async fn authorize_account(
    tx: &mut DbTx<'_>,
    actor: &AccountActor,
) -> Result<String, Error> {
    let pg = matches!(tx, DbTx::Postgres(_));
    let account_sql = if pg {
        "SELECT email FROM users WHERE user_id=$1 AND active=TRUE FOR SHARE"
    } else {
        "SELECT email FROM users WHERE user_id=$1 AND active=TRUE"
    };
    let email = scalar!(tx, String, account_sql, &actor.actor_id)
        .map_err(db_error)?
        .ok_or(Error::Forbidden)?;
    recheck_credential(tx, actor).await?;
    Ok(email)
}

/// Acquire the account row exclusively before the credential lock for settings writes.
pub(crate) async fn lock_account_settings(tx: &mut DbTx<'_>, actor: &AccountActor) -> Result<i64, Error> {
    let sql = if matches!(tx, DbTx::Postgres(_)) {
        "SELECT settings_revision FROM users WHERE user_id=$1 AND active=TRUE FOR UPDATE"
    } else {
        "SELECT settings_revision FROM users WHERE user_id=$1 AND active=TRUE"
    };
    let revision = scalar!(tx, i64, sql, &actor.actor_id).map_err(db_error)?.ok_or(Error::Forbidden)?;
    recheck_credential(tx, actor).await?;
    Ok(revision)
}

pub(crate) async fn recheck_credential(
    tx: &mut DbTx<'_>,
    actor: &AccountActor,
) -> Result<(), Error> {
    let sql = if matches!(tx, DbTx::Postgres(_)) {
        "SELECT credential_id FROM auth_credentials WHERE credential_id=$1 AND user_id=$2 AND revoked_at IS NULL AND expires_at>$3 FOR SHARE"
    } else {
        "SELECT credential_id FROM auth_credentials WHERE credential_id=$1 AND user_id=$2 AND revoked_at IS NULL AND expires_at>$3"
    };
    if actor.access_expires_at <= chrono::Utc::now().timestamp()
        || scalar!(
            tx,
            String,
            sql,
            &actor.credential_id,
            &actor.actor_id,
            chrono::Utc::now().timestamp()
        )
        .map_err(db_error)?
        .is_none()
    {
        return Err(Error::Forbidden);
    }
    Ok(())
}

/// Map internal failures to the same non-sensitive result for both adapters.
pub fn project(result: Result<Outcome, Error>) -> Outcome {
    match result {
        Ok(outcome) => outcome,
        Err(Error::Forbidden) => Outcome::Forbidden,
        Err(Error::Invalid(_)) => Outcome::Invalid,
        Err(Error::RequestIdentityConflict) => Outcome::RequestIdentityConflict,
        Err(Error::ResultExpired { resource_id }) => Outcome::ResultExpired { resource_id },
        Err(Error::Conflict { .. }) => Outcome::Conflict,
        Err(Error::Retryable) => Outcome::Retryable,
        Err(error) => {
            tracing::error!(%error, "invitation response failed");
            Outcome::Unavailable
        }
    }
}

#[derive(sqlx::FromRow)]
struct Invitation {
    workspace_id: String,
    role: String,
    status: String,
}

/// Apply one explicit recipient response using the installation's established identity.
pub async fn respond(
    pool: &DbPool,
    actor: &AccountActor,
    invitation_id: &str,
    action: InvitationAction,
    request_id: uuid::Uuid,
) -> Result<Outcome, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    let pg = matches!(tx, DbTx::Postgres(_));
    let email = authorize_account(&mut tx, actor).await?;
    let sql = if pg {
        "SELECT i.workspace_id,i.role,i.status FROM workspace_invitations i JOIN workspaces w ON w.workspace_id=i.workspace_id WHERE i.invitation_id=$1 AND lower(i.email)=lower($2) AND w.status='active' FOR UPDATE OF i,w"
    } else {
        "SELECT i.workspace_id,i.role,i.status FROM workspace_invitations i JOIN workspaces w ON w.workspace_id=i.workspace_id WHERE i.invitation_id=$1 AND lower(i.email)=lower($2) AND w.status='active'"
    };
    let invitation =
        krausen_core::dbtx_fetch_optional!(&mut tx, Invitation, sql, invitation_id, &email)
            .map_err(db_error)?;
    let Some(invitation) = invitation else {
        return Ok(Outcome::NotFound);
    };
    recheck_credential(&mut tx, actor).await?;
    let scoped = ActorContext {
        actor_id: actor.actor_id.clone(),
        workspace_id: invitation.workspace_id.clone(),
        credential_id: actor.credential_id.clone(),
        access_expires_at: actor.access_expires_at,
    };
    let request = request_id.to_string();
    let digest = krausen_core::canonical_json::sha256(
        &serde_json::json!({"invitation_id": invitation_id, "action": action}),
    )
    .map_err(|_| Error::Invalid("invitation response"))?;
    let key = RequestIdentity {
        actor: &scoped,
        operation: "invitation.respond.v1",
        request_id: &request,
        payload_digest: &digest,
    };
    if let Some(value) = crate::recover_request(&mut tx, &key).await? {
        let result = serde_json::from_value(value)?;
        tx.commit().await.map_err(boundary_error)?;
        return Ok(result);
    }
    if invitation.status != "pending" {
        return Ok(Outcome::Conflict);
    }
    let valid_sql = if pg {
        "SELECT invitation_id FROM workspace_invitations WHERE invitation_id=$1 AND status='pending' AND expires_at>clock_timestamp()"
    } else {
        "SELECT invitation_id FROM workspace_invitations WHERE invitation_id=$1 AND status='pending' AND julianday(expires_at)>julianday('now')"
    };
    if scalar!(&mut tx, String, valid_sql, invitation_id)
        .map_err(db_error)?
        .is_none()
    {
        return Ok(Outcome::Conflict);
    }
    if action == InvitationAction::Accept {
        if !matches!(
            invitation.role.as_str(),
            "workspace_admin" | "workspace_user"
        ) {
            return Err(Error::Invalid("invitation role"));
        }
        // Existing active memberships keep their current role. An inactive row
        // is not silently reactivated or promoted by an invitation response.
        let membership = scalar!(
            &mut tx,
            bool,
            "SELECT active FROM workspace_users WHERE workspace_id=$1 AND user_id=$2",
            &invitation.workspace_id,
            &actor.actor_id
        )
        .map_err(db_error)?;
        if membership == Some(false) {
            return Ok(Outcome::Conflict);
        }
        if membership.is_none() {
            query!(&mut tx, "INSERT INTO workspace_users (workspace_id,user_id,role,active) VALUES ($1,$2,$3,TRUE) ON CONFLICT (workspace_id,user_id) DO NOTHING", &invitation.workspace_id, &actor.actor_id, &invitation.role).map_err(db_error)?;
            if scalar!(
                &mut tx,
                bool,
                "SELECT active FROM workspace_users WHERE workspace_id=$1 AND user_id=$2",
                &invitation.workspace_id,
                &actor.actor_id
            )
            .map_err(db_error)?
                != Some(true)
            {
                return Ok(Outcome::Conflict);
            }
        }
    }
    let (status, outcome) = match action {
        InvitationAction::Accept => ("accepted", Outcome::Accepted),
        InvitationAction::Decline => ("declined", Outcome::Declined),
    };
    let update_sql = if pg {
        "UPDATE workspace_invitations SET status=$1,accepted_by_user_id=CASE WHEN $1='accepted' THEN $2 ELSE NULL END,accepted_at=CASE WHEN $1='accepted' THEN CURRENT_TIMESTAMP ELSE NULL END WHERE invitation_id=$3 AND status='pending' AND expires_at>clock_timestamp()"
    } else {
        "UPDATE workspace_invitations SET status=$1,accepted_by_user_id=CASE WHEN $1='accepted' THEN $2 ELSE NULL END,accepted_at=CASE WHEN $1='accepted' THEN CURRENT_TIMESTAMP ELSE NULL END WHERE invitation_id=$3 AND status='pending' AND julianday(expires_at)>julianday('now')"
    };
    if query!(&mut tx, update_sql, status, &actor.actor_id, invitation_id).map_err(db_error)? != 1 {
        return Ok(Outcome::Conflict);
    }
    crate::record_request(
        &mut tx,
        &key,
        &serde_json::to_value(&outcome)?,
        Some(invitation_id),
    )
    .await?;
    crate::record_change(&mut tx, &scoped, "invitation.respond.v1", invitation_id, 1).await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(outcome)
}
