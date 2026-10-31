// SPDX-License-Identifier: AGPL-3.0-or-later
//! Workspace ownership transfers under the shared transaction boundary.
use crate::{ActorContext, Error, RequestIdentity, authorize_workspace, boundary_error, db_error};
use krausen_core::{DbPool, DbTx};
use krausen_types::account::{OwnershipStartResult, OwnershipTransfer};

#[derive(sqlx::FromRow)]
struct TransferRow {
    transfer_id: String,
    workspace_id: String,
    from_user_id: String,
    from_user_email: String,
    to_user_id: String,
    to_user_email: String,
    status: String,
    created_at: String,
    expires_at: String,
}

/// The current owner and named recipient can read their own workspace transfers.
pub async fn list(
    pool: &DbPool,
    actor: &ActorContext,
) -> Result<krausen_types::account::BoundedList<OwnershipTransfer>, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    let sql = if matches!(tx, DbTx::Postgres(_)) {
        "SELECT workspace_id FROM workspaces WHERE workspace_id=$1 AND status='active' FOR SHARE"
    } else {
        "SELECT workspace_id FROM workspaces WHERE workspace_id=$1 AND status='active'"
    };
    if scalar!(&mut tx, String, sql, &actor.workspace_id)
        .map_err(db_error)?
        .is_none()
    {
        return Err(Error::Forbidden);
    }
    let sql = "SELECT t.transfer_id,t.workspace_id,t.from_user_id,source.email AS from_user_email,t.to_user_id,target.email AS to_user_email,t.status,CAST(t.created_at AS TEXT) AS created_at,CAST(t.expires_at AS TEXT) AS expires_at FROM ownership_transfers t JOIN users source ON source.user_id=t.from_user_id JOIN users target ON target.user_id=t.to_user_id JOIN workspaces w ON w.workspace_id=t.workspace_id WHERE t.workspace_id=$1 AND (t.to_user_id=$2 OR (t.from_user_id=$2 AND w.owner_user_id=$2)) ORDER BY t.transfer_id LIMIT 101";
    let rows = match &mut tx {
        DbTx::Postgres(t) => {
            sqlx::query_as::<_, TransferRow>(sql)
                .bind(&actor.workspace_id)
                .bind(&actor.actor_id)
                .fetch_all(&mut **t)
                .await
        }
        DbTx::Sqlite(t) => {
            sqlx::query_as::<_, TransferRow>(sql)
                .bind(&actor.workspace_id)
                .bind(&actor.actor_id)
                .fetch_all(&mut **t)
                .await
        }
    }
    .map_err(db_error)?;
    authorize_workspace(&mut tx, actor).await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(crate::bounded(
        rows.into_iter()
            .map(|r| OwnershipTransfer {
                transfer_id: r.transfer_id,
                workspace_id: r.workspace_id,
                from_user_id: r.from_user_id,
                from_user_email: r.from_user_email,
                to_user_id: r.to_user_id,
                to_user_email: r.to_user_email,
                status: r.status,
                created_at: r.created_at,
                expires_at: r.expires_at,
            })
            .collect(),
    ))
}

#[derive(sqlx::FromRow)]
struct Workspace {
    owner_user_id: String,
    revision: i64,
}
async fn lock_owner(tx: &mut DbTx<'_>, actor: &ActorContext) -> Result<Workspace, Error> {
    if authorize_workspace(tx, actor).await? != "workspace_admin" {
        return Err(Error::Forbidden);
    }
    let sql = if matches!(tx, DbTx::Postgres(_)) {
        "SELECT owner_user_id,revision FROM workspaces WHERE workspace_id=$1 AND status='active' FOR UPDATE"
    } else {
        "SELECT owner_user_id,revision FROM workspaces WHERE workspace_id=$1 AND status='active'"
    };
    let workspace = krausen_core::dbtx_fetch_optional!(tx, Workspace, sql, &actor.workspace_id)
        .map_err(db_error)?
        .ok_or(Error::Forbidden)?;
    if workspace.owner_user_id != actor.actor_id
        || authorize_workspace(tx, actor).await? != "workspace_admin"
    {
        return Err(Error::Forbidden);
    }
    Ok(workspace)
}

/// Start a transfer to an explicitly selected current member, with durable notice.
pub async fn start(
    pool: &DbPool,
    actor: &ActorContext,
    recipient: &str,
    expected: i64,
    request_id: uuid::Uuid,
    mail_available: bool,
) -> Result<OwnershipStartResult, Error> {
    if recipient == actor.actor_id {
        return Err(Error::Invalid("transfer recipient"));
    }
    let digest = krausen_core::canonical_json::sha256(
        &serde_json::json!({"recipient":recipient,"expected_revision":expected}),
    )
    .map_err(|_| Error::Invalid("transfer identity"))?;
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    // Lock both accounts in stable order before credentials and memberships.
    let accounts_sql = if matches!(tx, DbTx::Postgres(_)) {
        "SELECT user_id FROM users WHERE user_id IN ($1,$2) AND active=TRUE ORDER BY user_id FOR SHARE"
    } else {
        "SELECT user_id FROM users WHERE user_id IN ($1,$2) AND active=TRUE ORDER BY user_id"
    };
    let accounts = match &mut tx {
        DbTx::Postgres(t) => {
            sqlx::query_scalar::<_, String>(accounts_sql)
                .bind(&actor.actor_id)
                .bind(recipient)
                .fetch_all(&mut **t)
                .await
        }
        DbTx::Sqlite(t) => {
            sqlx::query_scalar::<_, String>(accounts_sql)
                .bind(&actor.actor_id)
                .bind(recipient)
                .fetch_all(&mut **t)
                .await
        }
    }
    .map_err(db_error)?;
    if accounts.len() != 2 {
        return Err(Error::Forbidden);
    }
    authorize_workspace(&mut tx, actor).await?;
    let member_sql = if matches!(tx, DbTx::Postgres(_)) {
        "SELECT m.user_id FROM workspace_users m WHERE m.workspace_id=$1 AND m.user_id=$2 AND m.active=TRUE FOR SHARE OF m"
    } else {
        "SELECT m.user_id FROM workspace_users m WHERE m.workspace_id=$1 AND m.user_id=$2 AND m.active=TRUE"
    };
    if scalar!(&mut tx, String, member_sql, &actor.workspace_id, recipient)
        .map_err(db_error)?
        .is_none()
    {
        return Err(Error::Forbidden);
    }
    let workspace = lock_owner(&mut tx, actor).await?;
    let request = request_id.to_string();
    let key = RequestIdentity {
        actor,
        operation: "workspace.ownership.start.v1",
        request_id: &request,
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
    if workspace.revision != expected {
        return Err(Error::Conflict {
            expected,
            actual: workspace.revision,
        });
    }
    let pending_sql = if matches!(tx, DbTx::Postgres(_)) {
        "SELECT COUNT(*) FROM ownership_transfers WHERE workspace_id=$1 AND status='pending' AND expires_at>CURRENT_TIMESTAMP"
    } else {
        "SELECT COUNT(*) FROM ownership_transfers WHERE workspace_id=$1 AND status='pending' AND julianday(expires_at)>julianday('now')"
    };
    if scalar!(&mut tx, i64, pending_sql, &actor.workspace_id)
        .map_err(db_error)?
        .unwrap_or_default()
        != 0
    {
        return Err(Error::Invalid("transfer already pending"));
    }
    let transfer_id = uuid::Uuid::new_v4().to_string();
    let expires = chrono::Utc::now() + chrono::Duration::days(7);
    query!(&mut tx, "INSERT INTO ownership_transfers (transfer_id,workspace_id,from_user_id,to_user_id,status,expires_at) VALUES ($1,$2,$3,$4,'pending',$5)", &transfer_id, &actor.workspace_id, &actor.actor_id, recipient, expires).map_err(db_error)?;
    let revision = workspace.revision + 1;
    query!(
        &mut tx,
        "UPDATE workspaces SET revision=$1 WHERE workspace_id=$2 AND revision=$3",
        revision,
        &actor.workspace_id,
        workspace.revision
    )
    .map_err(db_error)?;
    crate::enqueue_outbox(
        &mut tx,
        &actor.workspace_id,
        "workspace.ownership.notify.v1",
        &serde_json::json!({"transfer_id":transfer_id}),
    )
    .await?;
    let result = OwnershipStartResult {
        transfer_id: transfer_id.clone(),
        revision,
        delivery: "pending".into(),
    };
    crate::record_request(
        &mut tx,
        &key,
        &serde_json::to_value(&result)?,
        Some(&transfer_id),
    )
    .await?;
    crate::record_change(
        &mut tx,
        actor,
        "workspace.ownership.start.v1",
        &transfer_id,
        revision,
    )
    .await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(result)
}

/// Cancel an uncompleted transfer only while its initiator remains owner.
pub async fn cancel(
    pool: &DbPool,
    actor: &ActorContext,
    transfer_id: &str,
    expected: i64,
    request_id: uuid::Uuid,
) -> Result<i64, Error> {
    let digest = krausen_core::canonical_json::sha256(
        &serde_json::json!({"transfer_id":transfer_id,"expected_revision":expected}),
    )
    .map_err(|_| Error::Invalid("transfer identity"))?;
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    let workspace = lock_owner(&mut tx, actor).await?;
    let request = request_id.to_string();
    let key = RequestIdentity {
        actor,
        operation: "workspace.ownership.cancel.v1",
        request_id: &request,
        payload_digest: &digest,
    };
    if let Some(result) = crate::recover_request(&mut tx, &key).await? {
        let revision = serde_json::from_value(result)?;
        tx.commit().await.map_err(boundary_error)?;
        return Ok(revision);
    }
    if expected != workspace.revision {
        return Err(Error::Conflict {
            expected,
            actual: workspace.revision,
        });
    }
    let affected = query!(&mut tx, "UPDATE ownership_transfers SET status='cancelled',completed_at=CURRENT_TIMESTAMP WHERE transfer_id=$1 AND workspace_id=$2 AND from_user_id=$3 AND status='pending'", transfer_id, &actor.workspace_id, &actor.actor_id).map_err(db_error)?;
    if affected != 1 {
        return Err(Error::Forbidden);
    }
    let revision = workspace.revision + 1;
    query!(
        &mut tx,
        "UPDATE workspaces SET revision=$1 WHERE workspace_id=$2 AND revision=$3",
        revision,
        &actor.workspace_id,
        workspace.revision
    )
    .map_err(db_error)?;
    crate::record_request(
        &mut tx,
        &key,
        &serde_json::json!(revision),
        Some(transfer_id),
    )
    .await?;
    crate::record_change(
        &mut tx,
        actor,
        "workspace.ownership.cancel.v1",
        transfer_id,
        revision,
    )
    .await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(revision)
}

pub fn project_start(
    result: Result<OwnershipStartResult, Error>,
) -> krausen_types::account::OwnershipStartOutcome {
    use krausen_types::account::OwnershipStartOutcome as Outcome;
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
            tracing::error!(%error,"ownership transfer start failed");
            Outcome::Unavailable
        }
    }
}

#[derive(sqlx::FromRow)]
struct Parties {
    from_user_id: String,
    to_user_id: String,
    workspace_id: String,
    status: String,
}

/// Lock the two named accounts in stable order, then credential and memberships.
async fn lock_parties(
    tx: &mut DbTx<'_>,
    actor: &ActorContext,
    other_id: &str,
) -> Result<(), Error> {
    let pg = matches!(tx, DbTx::Postgres(_));
    let sql = if pg {
        "SELECT user_id FROM users WHERE user_id IN ($1,$2) AND active=TRUE ORDER BY user_id FOR SHARE"
    } else {
        "SELECT user_id FROM users WHERE user_id IN ($1,$2) AND active=TRUE ORDER BY user_id"
    };
    let accounts = match &mut *tx {
        DbTx::Postgres(t) => {
            sqlx::query_scalar::<_, String>(sql)
                .bind(&actor.actor_id)
                .bind(other_id)
                .fetch_all(&mut **t)
                .await
        }
        DbTx::Sqlite(t) => {
            sqlx::query_scalar::<_, String>(sql)
                .bind(&actor.actor_id)
                .bind(other_id)
                .fetch_all(&mut **t)
                .await
        }
    }
    .map_err(db_error)?;
    if accounts.len() != 2 {
        return Err(Error::Forbidden);
    }
    crate::authorize_account(tx, actor).await?;
    let sql = if pg {
        "SELECT user_id FROM workspace_users WHERE workspace_id=$1 AND user_id IN ($2,$3) AND active=TRUE ORDER BY user_id FOR SHARE"
    } else {
        "SELECT user_id FROM workspace_users WHERE workspace_id=$1 AND user_id IN ($2,$3) AND active=TRUE ORDER BY user_id"
    };
    let members = match &mut *tx {
        DbTx::Postgres(t) => {
            sqlx::query_scalar::<_, String>(sql)
                .bind(&actor.workspace_id)
                .bind(&actor.actor_id)
                .bind(other_id)
                .fetch_all(&mut **t)
                .await
        }
        DbTx::Sqlite(t) => {
            sqlx::query_scalar::<_, String>(sql)
                .bind(&actor.workspace_id)
                .bind(&actor.actor_id)
                .bind(other_id)
                .fetch_all(&mut **t)
                .await
        }
    }
    .map_err(db_error)?;
    if members.len() != 2 {
        return Err(Error::Forbidden);
    }
    Ok(())
}

/// Start an account- and action-bound mailbox confirmation for a named recipient.
pub async fn begin_response(
    pool: &DbPool,
    actor: &ActorContext,
    transfer_id: &str,
    accept: bool,
    expected: i64,
    request_id: uuid::Uuid,
    mail_available: bool,
) -> Result<krausen_types::account::PendingInteraction, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    let initial = krausen_core::dbtx_fetch_optional!(&mut tx, Parties, "SELECT from_user_id,to_user_id,workspace_id,status FROM ownership_transfers WHERE transfer_id=$1 AND to_user_id=$2 AND workspace_id=$3",transfer_id,&actor.actor_id,&actor.workspace_id).map_err(db_error)?.ok_or(Error::Forbidden)?;
    if initial.to_user_id != actor.actor_id || initial.workspace_id != actor.workspace_id {
        return Err(Error::Forbidden);
    }
    lock_parties(&mut tx, actor, &initial.from_user_id).await?;
    let pg = matches!(tx, DbTx::Postgres(_));
    let sql = if pg {
        "SELECT owner_user_id,revision FROM workspaces WHERE workspace_id=$1 AND status='active' FOR SHARE"
    } else {
        "SELECT owner_user_id,revision FROM workspaces WHERE workspace_id=$1 AND status='active'"
    };
    let workspace =
        krausen_core::dbtx_fetch_optional!(&mut tx, Workspace, sql, &actor.workspace_id)
            .map_err(db_error)?
            .ok_or(Error::Forbidden)?;
    if workspace.owner_user_id != initial.from_user_id {
        return Err(Error::Forbidden);
    }
    let action = if accept {
        "ownership_accept"
    } else {
        "ownership_decline"
    };
    let digest = krausen_core::canonical_json::sha256(&serde_json::json!({"transfer_id":transfer_id,"action":action,"expected_revision":expected})).map_err(|_| Error::Invalid("transfer proof identity"))?;
    let request = request_id.to_string();
    let key = RequestIdentity {
        actor,
        operation: "workspace.ownership.response.begin.v1",
        request_id: &request,
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
    if expected != workspace.revision {
        return Err(Error::Conflict {
            expected,
            actual: workspace.revision,
        });
    }
    if initial.status != "pending" {
        return Err(Error::Forbidden);
    }
    let sql = if pg {
        "SELECT transfer_id FROM ownership_transfers WHERE transfer_id=$1 AND to_user_id=$2 AND workspace_id=$3 AND status='pending' AND expires_at>CURRENT_TIMESTAMP FOR SHARE"
    } else {
        "SELECT transfer_id FROM ownership_transfers WHERE transfer_id=$1 AND to_user_id=$2 AND workspace_id=$3 AND status='pending' AND julianday(expires_at)>julianday('now')"
    };
    if scalar!(
        &mut tx,
        String,
        sql,
        transfer_id,
        &actor.actor_id,
        &actor.workspace_id
    )
    .map_err(db_error)?
    .is_none()
    {
        return Err(Error::Forbidden);
    }
    let email = scalar!(
        &mut tx,
        String,
        "SELECT email FROM users WHERE user_id=$1",
        &actor.actor_id
    )
    .map_err(db_error)?
    .ok_or(Error::Forbidden)?;
    let proof_id = uuid::Uuid::new_v4().to_string();
    let expires = chrono::Utc::now().timestamp() + 900;
    query!(&mut tx,"INSERT INTO account_action_proofs (proof_id,account_id,credential_id,action,target_id,workspace_id,expected_revision,recipient_email,expires_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",&proof_id,&actor.actor_id,&actor.credential_id,action,transfer_id,&actor.workspace_id,expected,&email,expires).map_err(db_error)?;
    let outbox = crate::enqueue_outbox(
        &mut tx,
        &actor.workspace_id,
        "account.invitation.proof.v1",
        &serde_json::json!({"proof_id":proof_id}),
    )
    .await?;
    query!(
        &mut tx,
        "UPDATE account_action_proofs SET outbox_id=$1 WHERE proof_id=$2",
        &outbox,
        &proof_id
    )
    .map_err(db_error)?;
    let result = krausen_types::account::PendingInteraction {
        interaction_id: proof_id.clone(),
        expires_at: expires,
        next_action: "check_email".into(),
    };
    crate::record_request(
        &mut tx,
        &key,
        &serde_json::to_value(&result)?,
        Some(&proof_id),
    )
    .await?;
    crate::record_change(
        &mut tx,
        actor,
        "workspace.ownership.response.begin.v1",
        &proof_id,
        workspace.revision,
    )
    .await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(result)
}

#[derive(sqlx::FromRow)]
struct ProofParties {
    from_user_id: String,
    workspace_id: String,
    consumed_at: Option<i64>,
}

/// Prepare lock ordering before the generic proof loader locks credentials or resources.
pub(crate) async fn prepare_proof(
    tx: &mut DbTx<'_>,
    actor: &ActorContext,
    proof_id: &str,
) -> Result<bool, Error> {
    let initial = krausen_core::dbtx_fetch_optional!(tx,ProofParties,"SELECT t.from_user_id,t.workspace_id,p.consumed_at FROM account_action_proofs p JOIN ownership_transfers t ON t.transfer_id=p.target_id AND t.workspace_id=p.workspace_id WHERE p.proof_id=$1 AND p.account_id=$2 AND p.action IN ('ownership_accept','ownership_decline') AND t.to_user_id=$2",proof_id,&actor.actor_id).map_err(db_error)?;
    if let Some(initial) = initial {
        if initial.consumed_at.is_none() {
            let scoped = ActorContext {
                workspace_id: initial.workspace_id,
                ..actor.clone()
            };
            lock_parties(tx, &scoped, &initial.from_user_id).await?;
        }
        return Ok(true);
    }
    Ok(false)
}

/// Verify the transfer's still-current named parties and workspace under held locks.
pub(crate) async fn preview_in_tx(
    tx: &mut DbTx<'_>,
    actor: &ActorContext,
    proof: &crate::invitation::Proof,
) -> Result<String, Error> {
    let workspace_id = proof.workspace_id.as_deref().ok_or(Error::Forbidden)?;
    let scoped = ActorContext {
        workspace_id: workspace_id.to_owned(),
        ..actor.clone()
    };
    authorize_workspace(tx, &scoped).await?;
    #[derive(sqlx::FromRow)]
    struct Current {
        name: Option<String>,
        owner_user_id: String,
        revision: i64,
    }
    let sql = if matches!(tx, DbTx::Postgres(_)) {
        "SELECT name,owner_user_id,revision FROM workspaces WHERE workspace_id=$1 AND status='active' FOR SHARE"
    } else {
        "SELECT name,owner_user_id,revision FROM workspaces WHERE workspace_id=$1 AND status='active'"
    };
    let current = krausen_core::dbtx_fetch_optional!(tx, Current, sql, workspace_id)
        .map_err(db_error)?
        .ok_or(Error::Forbidden)?;
    let role = scalar!(
        tx,
        String,
        "SELECT role FROM workspace_users WHERE workspace_id=$1 AND user_id=$2 AND active=TRUE",
        workspace_id,
        &current.owner_user_id
    )
    .map_err(db_error)?
    .ok_or(Error::Forbidden)?;
    if role != "workspace_admin" {
        return Err(Error::Forbidden);
    }
    if proof.expected_revision != Some(current.revision) {
        return Err(Error::Conflict {
            expected: proof.expected_revision.unwrap_or(-1),
            actual: current.revision,
        });
    }
    let sql = if matches!(tx, DbTx::Postgres(_)) {
        "SELECT from_user_id,to_user_id,workspace_id,status FROM ownership_transfers WHERE transfer_id=$1 AND workspace_id=$2 AND to_user_id=$3 AND from_user_id=$4 AND status='pending' AND expires_at>CURRENT_TIMESTAMP FOR SHARE"
    } else {
        "SELECT from_user_id,to_user_id,workspace_id,status FROM ownership_transfers WHERE transfer_id=$1 AND workspace_id=$2 AND to_user_id=$3 AND from_user_id=$4 AND status='pending' AND julianday(expires_at)>julianday('now')"
    };
    let transfer = krausen_core::dbtx_fetch_optional!(
        tx,
        Parties,
        sql,
        &proof.target_id,
        workspace_id,
        &actor.actor_id,
        &current.owner_user_id
    )
    .map_err(db_error)?
    .ok_or(Error::Forbidden)?;
    if transfer.status != "pending" {
        return Err(Error::Forbidden);
    }
    Ok(current.name.unwrap_or_else(|| "Unnamed Workspace".into()))
}

/// Apply a previously verified ownership proof on the caller's held transaction.
pub(crate) async fn complete_in_tx(
    tx: &mut DbTx<'_>,
    actor: &ActorContext,
    proof: &crate::invitation::Proof,
) -> Result<krausen_types::account::InvitationProofOutcome, Error> {
    // Generic confirmation already holds the workspace exclusively; this check
    // takes the same locks in this transaction and rejects stale/changed parties.
    let _ = preview_in_tx(tx, actor, proof).await?;
    let workspace_id = proof.workspace_id.as_deref().ok_or(Error::Forbidden)?;
    let revision = proof.expected_revision.ok_or(Error::Forbidden)?;
    let (status, outcome) = match proof.action.as_str() {
        "ownership_accept" => {
            let changed=query!(tx,"UPDATE workspaces SET owner_user_id=$1,revision=revision+1 WHERE workspace_id=$2 AND revision=$3",&actor.actor_id,workspace_id,revision).map_err(db_error)?;
            if changed != 1 {
                return Err(Error::Retryable);
            }
            let changed=query!(tx,"UPDATE workspace_users SET role='workspace_admin' WHERE workspace_id=$1 AND user_id=$2 AND active=TRUE",workspace_id,&actor.actor_id).map_err(db_error)?;
            if changed != 1 {
                return Err(Error::Forbidden);
            }
            (
                "accepted",
                krausen_types::account::InvitationProofOutcome::OwnershipAccepted,
            )
        }
        "ownership_decline" => {
            let changed = query!(
                tx,
                "UPDATE workspaces SET revision=revision+1 WHERE workspace_id=$1 AND revision=$2",
                workspace_id,
                revision
            )
            .map_err(db_error)?;
            if changed != 1 {
                return Err(Error::Retryable);
            }
            (
                "declined",
                krausen_types::account::InvitationProofOutcome::OwnershipDeclined,
            )
        }
        _ => return Err(Error::Forbidden),
    };
    let changed=query!(tx,"UPDATE ownership_transfers SET status=$1,completed_at=CURRENT_TIMESTAMP WHERE transfer_id=$2 AND workspace_id=$3 AND to_user_id=$4 AND status='pending'",status,&proof.target_id,workspace_id,&actor.actor_id).map_err(db_error)?;
    if changed != 1 {
        return Err(Error::Retryable);
    }
    Ok(outcome)
}

#[derive(sqlx::FromRow)]
struct DetailRow {
    transfer_id: String,
    workspace_id: String,
    workspace_name: Option<String>,
    from_user_email: String,
    expires_at: String,
    status: String,
    revision: i64,
}

/// Read transfer details only for its explicitly named recipient or current owner.
pub async fn get(
    pool: &DbPool,
    actor: &ActorContext,
    transfer_id: &str,
) -> Result<krausen_types::account::OwnershipDetail, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    crate::authorize_account(&mut tx, actor).await?;
    let initial=krausen_core::dbtx_fetch_optional!(&mut tx,Parties,"SELECT from_user_id,to_user_id,workspace_id,status FROM ownership_transfers WHERE transfer_id=$1 AND (to_user_id=$2 OR from_user_id=$2)",transfer_id,&actor.actor_id).map_err(db_error)?.ok_or(Error::Forbidden)?;
    let scoped = ActorContext {
        workspace_id: initial.workspace_id,
        ..actor.clone()
    };
    authorize_workspace(&mut tx, &scoped).await?;
    let sql = if matches!(tx, DbTx::Postgres(_)) {
        "SELECT t.transfer_id,t.workspace_id,w.name AS workspace_name,u.email AS from_user_email,CAST(t.expires_at AS TEXT) AS expires_at,t.status,w.revision FROM ownership_transfers t JOIN workspaces w ON w.workspace_id=t.workspace_id JOIN users u ON u.user_id=t.from_user_id WHERE t.transfer_id=$1 AND w.status='active' AND t.status='pending' AND t.expires_at>CURRENT_TIMESTAMP AND (t.to_user_id=$2 OR (t.from_user_id=$2 AND w.owner_user_id=$2)) FOR SHARE OF t,w"
    } else {
        "SELECT t.transfer_id,t.workspace_id,w.name AS workspace_name,u.email AS from_user_email,CAST(t.expires_at AS TEXT) AS expires_at,t.status,w.revision FROM ownership_transfers t JOIN workspaces w ON w.workspace_id=t.workspace_id JOIN users u ON u.user_id=t.from_user_id WHERE t.transfer_id=$1 AND w.status='active' AND t.status='pending' AND julianday(t.expires_at)>julianday('now') AND (t.to_user_id=$2 OR (t.from_user_id=$2 AND w.owner_user_id=$2))"
    };
    let row =
        krausen_core::dbtx_fetch_optional!(&mut tx, DetailRow, sql, transfer_id, &actor.actor_id)
            .map_err(db_error)?
            .ok_or(Error::Forbidden)?;
    authorize_workspace(&mut tx, &scoped).await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(krausen_types::account::OwnershipDetail {
        transfer_id: row.transfer_id,
        workspace_id: row.workspace_id,
        workspace_name: row
            .workspace_name
            .unwrap_or_else(|| "Unnamed Workspace".into()),
        from_user_email: row.from_user_email,
        expires_at: row.expires_at,
        status: row.status,
        revision: row.revision,
    })
}

pub fn project_response(
    result: Result<krausen_types::account::PendingInteraction, Error>,
) -> krausen_types::account::OwnershipResponseOutcome {
    use krausen_types::account::OwnershipResponseOutcome as Outcome;
    match result {
        Ok(pending) => Outcome::Pending(pending),
        Err(Error::Conflict { expected, actual }) => Outcome::Conflict { expected, actual },
        Err(Error::RequestIdentityConflict) => Outcome::RequestIdentityConflict,
        Err(Error::ResultExpired { .. }) => Outcome::ResultExpired,
        Err(Error::Forbidden) => Outcome::Forbidden,
        Err(Error::Invalid(_)) => Outcome::Invalid,
        Err(Error::Retryable) => Outcome::Retryable,
        Err(Error::DeliveryUnavailable) => Outcome::DeliveryUnavailable,
        Err(error) => {
            tracing::error!(%error,"ownership response start failed");
            Outcome::Unavailable
        }
    }
}
