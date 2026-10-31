// SPDX-License-Identifier: AGPL-3.0-or-later
//! Recipient-bound invitation proof and atomic completion.
use crate::{
    ActorContext, Error, RequestIdentity, authorize_account, boundary_error, db_error,
    enqueue_outbox, record_change, record_request, recover_request,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use krausen_core::{DbPool, DbTx};
use krausen_types::account::{InvitationProofOutcome, PendingInteraction};
use sha2::{Digest, Sha256};

pub use krausen_types::account::InvitationAction;
/// Proof links may be delivered only to a configured HTTPS origin, or loopback in development.
pub fn delivery_available(smtp_configured: bool, base_url: &str) -> bool {
    if !smtp_configured {
        return false;
    }
    let Ok(url) = url::Url::parse(base_url) else {
        return false;
    };
    if !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return false;
    }
    url.scheme() == "https" && url.host_str().is_some()
        || url.scheme() == "http"
            && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
}

#[derive(sqlx::FromRow)]
struct Invitation {
    workspace_id: String,
    email: String,
    role: String,
}
#[derive(sqlx::FromRow)]
pub struct Proof {
    pub proof_id: String,
    pub account_id: String,
    pub credential_id: String,
    pub action: String,
    pub target_id: String,
    pub workspace_id: Option<String>,
    pub expected_revision: Option<i64>,
    pub recipient_email: String,
    pub outbox_id: Option<String>,
    pub expires_at: i64,
    pub consumed_at: Option<i64>,
    pub completion_credential_id: Option<String>,
    pub result: Option<String>,
}

async fn eligible(
    tx: &mut DbTx<'_>,
    actor: &ActorContext,
    invitation_id: &str,
) -> Result<Invitation, Error> {
    let pg = matches!(tx, DbTx::Postgres(_));
    let sql = if pg {
        "SELECT i.workspace_id,i.email,i.role FROM workspace_invitations i JOIN users u ON u.user_id=$1 JOIN workspaces w ON w.workspace_id=i.workspace_id JOIN workspace_users inviter ON inviter.workspace_id=i.workspace_id AND inviter.user_id=i.invited_by_user_id JOIN users issuer ON issuer.user_id=i.invited_by_user_id WHERE i.invitation_id=$2 AND lower(i.email)=lower(u.email) AND i.status='pending' AND i.expires_at>CURRENT_TIMESTAMP AND w.status='active' AND inviter.active=TRUE AND inviter.role='workspace_admin' AND issuer.active=TRUE FOR UPDATE OF i FOR SHARE OF inviter,issuer,w"
    } else {
        "SELECT i.workspace_id,i.email,i.role FROM workspace_invitations i JOIN users u ON u.user_id=$1 JOIN workspaces w ON w.workspace_id=i.workspace_id JOIN workspace_users inviter ON inviter.workspace_id=i.workspace_id AND inviter.user_id=i.invited_by_user_id JOIN users issuer ON issuer.user_id=i.invited_by_user_id WHERE i.invitation_id=$2 AND lower(i.email)=lower(u.email) AND i.status='pending' AND julianday(i.expires_at)>julianday('now') AND w.status='active' AND inviter.active=1 AND inviter.role='workspace_admin' AND issuer.active=1"
    };
    krausen_core::dbtx_fetch_optional!(tx, Invitation, sql, &actor.actor_id, invitation_id)
        .map_err(db_error)?
        .ok_or(Error::Forbidden)
}

/// Start mailbox proof; the pending result never grants membership.
pub async fn begin(
    pool: &DbPool,
    actor: &ActorContext,
    invitation_id: &str,
    discovery_id: &str,
    action: InvitationAction,
    request_id: uuid::Uuid,
    mail_available: bool,
) -> Result<PendingInteraction, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_account(&mut tx, actor).await?;
    verify_discovery(&mut tx, actor, discovery_id).await?;
    let digest = format!(
        "{:x}",
        Sha256::digest(
            format!(
                "invitation-proof.v1\0{}\0{}",
                action.as_str(),
                invitation_id
            )
            .as_bytes()
        )
    );
    let request = request_id.to_string();
    let key = RequestIdentity {
        actor,
        operation: "invitation.proof.begin.v1",
        request_id: &request,
        payload_digest: &digest,
    };
    if let Some(result) = recover_request(&mut tx, &key).await? {
        let result = serde_json::from_value(result)?;
        tx.commit().await.map_err(boundary_error)?;
        return Ok(result);
    }
    if !mail_available {
        return Err(Error::DeliveryUnavailable);
    }
    let invitation = eligible(&mut tx, actor, invitation_id).await?;
    if invitation.workspace_id != actor.workspace_id {
        return Err(Error::Forbidden);
    }
    if !matches!(
        invitation.role.as_str(),
        "workspace_user" | "workspace_admin"
    ) {
        return Err(Error::Invalid("invalid invitation role"));
    }
    authorize_account(&mut tx, actor).await?;
    verify_discovery(&mut tx, actor, discovery_id).await?;
    let proof_id = uuid::Uuid::new_v4().to_string();
    let expires_at = chrono::Utc::now().timestamp() + 900;
    krausen_core::dbtx_execute!(&mut tx, "INSERT INTO account_action_proofs (proof_id,account_id,credential_id,action,target_id,workspace_id,recipient_email,expires_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)", &proof_id, &actor.actor_id, &actor.credential_id, action.as_str(), invitation_id, &invitation.workspace_id, &invitation.email, expires_at).map_err(db_error)?;
    let pending = PendingInteraction {
        interaction_id: proof_id.clone(),
        expires_at,
        next_action: "check_email".into(),
    };
    let outbox_id = enqueue_outbox(
        &mut tx,
        &invitation.workspace_id,
        "account.invitation.proof.v1",
        &serde_json::json!({"proof_id":proof_id}),
    )
    .await?;
    krausen_core::dbtx_execute!(
        &mut tx,
        "UPDATE account_action_proofs SET outbox_id=$1 WHERE proof_id=$2",
        &outbox_id,
        &proof_id
    )
    .map_err(db_error)?;
    record_request(
        &mut tx,
        &key,
        &serde_json::to_value(&pending)?,
        Some(invitation_id),
    )
    .await?;
    record_change(
        &mut tx,
        actor,
        "invitation.response.proof.pending.v1",
        &proof_id,
        0,
    )
    .await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(pending)
}

/// Derive the delivery token without storing its secret in outbox or retry records.
pub fn delivery_token(secret: &[u8], proof: &Proof) -> Result<String, Error> {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret)
        .map_err(|_| Error::Invalid("invalid installation key"))?;
    for part in [
        "account-interaction-proof.v1",
        &proof.proof_id,
        &proof.account_id,
        &proof.credential_id,
        &proof.action,
        &proof.target_id,
        proof.workspace_id.as_deref().unwrap_or(""),
        &proof.recipient_email,
    ] {
        mac.update(&(part.len() as u64).to_be_bytes());
        mac.update(part.as_bytes());
    }
    mac.update(&proof.expires_at.to_be_bytes());
    mac.update(&proof.expected_revision.unwrap_or(-1).to_be_bytes());
    Ok(URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()))
}

/// Verify a mailbox link and show its bound action before any confirmation.
pub async fn preview(
    pool: &DbPool,
    actor: &ActorContext,
    proof_id: &str,
    token: &str,
    secret: &[u8],
) -> Result<krausen_types::account::ProofPreview, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    let prepared_ownership = crate::ownership::prepare_proof(&mut tx, actor, proof_id).await?;
    authorize_account(&mut tx, actor).await?;
    let initial = krausen_core::dbtx_fetch_optional!(
        &mut tx,
        Proof,
        "SELECT * FROM account_action_proofs WHERE proof_id=$1",
        proof_id
    )
    .map_err(db_error)?
    .ok_or(Error::Forbidden)?;
    if initial.account_id != actor.actor_id {
        return Err(Error::Forbidden);
    }
    #[derive(sqlx::FromRow)]
    struct Credential {
        credential_id: String,
    }
    let credential_sql = if pool.is_postgres() {
        "SELECT credential_id FROM auth_credentials WHERE credential_id=$1 AND user_id=$2 AND revoked_at IS NULL AND expires_at>$3 FOR SHARE"
    } else {
        "SELECT credential_id FROM auth_credentials WHERE credential_id=$1 AND user_id=$2 AND revoked_at IS NULL AND expires_at>$3"
    };
    let credential = krausen_core::dbtx_fetch_optional!(
        &mut tx,
        Credential,
        credential_sql,
        &initial.credential_id,
        &actor.actor_id,
        chrono::Utc::now().timestamp()
    )
    .map_err(db_error)?
    .ok_or(Error::Forbidden)?;
    if credential.credential_id != initial.credential_id {
        return Err(Error::Forbidden);
    }
    let proof_sql = if pool.is_postgres() {
        "SELECT * FROM account_action_proofs WHERE proof_id=$1 FOR SHARE"
    } else {
        "SELECT * FROM account_action_proofs WHERE proof_id=$1"
    };
    let proof = krausen_core::dbtx_fetch_optional!(&mut tx, Proof, proof_sql, proof_id)
        .map_err(db_error)?
        .ok_or(Error::Forbidden)?;
    use subtle::ConstantTimeEq;
    if proof.account_id != actor.actor_id
        || proof.credential_id != initial.credential_id
        || proof.consumed_at.is_some()
        || proof.expires_at <= chrono::Utc::now().timestamp()
        || !bool::from(
            delivery_token(secret, &proof)?
                .as_bytes()
                .ct_eq(token.as_bytes()),
        )
        || !account_email(&mut tx, actor)
            .await?
            .eq_ignore_ascii_case(&proof.recipient_email)
    {
        return Err(Error::Forbidden);
    }
    let workspace_name = match proof.action.as_str() {
        "discover" if proof.target_id == actor.actor_id && proof.workspace_id.is_none() => None,
        "accept" | "decline" => {
            let invitation = eligible(&mut tx, actor, &proof.target_id).await?;
            if proof.workspace_id.as_deref() != Some(invitation.workspace_id.as_str())
                || !invitation
                    .email
                    .eq_ignore_ascii_case(&proof.recipient_email)
            {
                return Err(Error::Forbidden);
            }
            #[derive(sqlx::FromRow)]
            struct Name {
                name: Option<String>,
            }
            let row = krausen_core::dbtx_fetch_optional!(
                &mut tx,
                Name,
                "SELECT name FROM workspaces WHERE workspace_id=$1 AND status='active'",
                &invitation.workspace_id
            )
            .map_err(db_error)?
            .ok_or(Error::Forbidden)?;
            Some(row.name.unwrap_or_else(|| "Unnamed Workspace".into()))
        }
        "ownership_accept" | "ownership_decline" if prepared_ownership => {
            Some(crate::ownership::preview_in_tx(&mut tx, actor, &proof).await?)
        }
        _ => return Err(Error::Forbidden),
    };
    authorize_account(&mut tx, actor).await?;
    let credential = krausen_core::dbtx_fetch_optional!(
        &mut tx,
        Credential,
        credential_sql,
        &proof.credential_id,
        &actor.actor_id,
        chrono::Utc::now().timestamp()
    )
    .map_err(db_error)?
    .ok_or(Error::Forbidden)?;
    if credential.credential_id != proof.credential_id
        || proof.expires_at <= chrono::Utc::now().timestamp()
    {
        return Err(Error::Forbidden);
    }
    tx.commit().await.map_err(boundary_error)?;
    Ok(krausen_types::account::ProofPreview {
        action: proof.action,
        workspace_name,
    })
}

/// Complete only with the mailbox token and a verified account session. The
/// initiator credential remains bound and must still be live in this transaction.
pub async fn complete(
    pool: &DbPool,
    actor: &ActorContext,
    proof_id: &str,
    token: &str,
    request_id: uuid::Uuid,
    secret: &[u8],
) -> Result<InvitationProofOutcome, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    let prepared_ownership = crate::ownership::prepare_proof(&mut tx, actor, proof_id).await?;
    authorize_account(&mut tx, actor).await?;
    let pg = pool.is_postgres();
    // Read only to discover the initiating credential, then lock account and
    // credential before acquiring the mutable proof/resource lock.
    let initial = krausen_core::dbtx_fetch_optional!(
        &mut tx,
        Proof,
        "SELECT * FROM account_action_proofs WHERE proof_id=$1",
        proof_id
    )
    .map_err(db_error)?
    .ok_or(Error::Forbidden)?;
    if initial.account_id != actor.actor_id {
        return Err(Error::Forbidden);
    }
    #[derive(sqlx::FromRow)]
    struct Credential {
        credential_id: String,
    }
    let credential_sql = if pg {
        "SELECT credential_id FROM auth_credentials WHERE credential_id=$1 AND user_id=$2 AND revoked_at IS NULL AND expires_at>$3 FOR SHARE"
    } else {
        "SELECT credential_id FROM auth_credentials WHERE credential_id=$1 AND user_id=$2 AND revoked_at IS NULL AND expires_at>$3"
    };
    let credential = krausen_core::dbtx_fetch_optional!(
        &mut tx,
        Credential,
        credential_sql,
        &initial.credential_id,
        &actor.actor_id,
        chrono::Utc::now().timestamp()
    )
    .map_err(db_error)?
    .ok_or(Error::Forbidden)?;
    if credential.credential_id != initial.credential_id {
        return Err(Error::Forbidden);
    }
    let proof_sql = if pg {
        "SELECT * FROM account_action_proofs WHERE proof_id=$1 FOR UPDATE"
    } else {
        "SELECT * FROM account_action_proofs WHERE proof_id=$1"
    };
    let proof = krausen_core::dbtx_fetch_optional!(&mut tx, Proof, proof_sql, proof_id)
        .map_err(db_error)?
        .ok_or(Error::Forbidden)?;
    if proof.account_id != actor.actor_id || proof.credential_id != initial.credential_id {
        return Err(Error::Forbidden);
    }
    use subtle::ConstantTimeEq;
    let expected = delivery_token(secret, &proof)?;
    if !bool::from(expected.as_bytes().ct_eq(token.as_bytes())) {
        return Err(Error::Forbidden);
    }
    let still_live = krausen_core::dbtx_fetch_optional!(
        &mut tx,
        Credential,
        credential_sql,
        &proof.credential_id,
        &actor.actor_id,
        chrono::Utc::now().timestamp()
    )
    .map_err(db_error)?
    .ok_or(Error::Forbidden)?;
    if still_live.credential_id != proof.credential_id {
        return Err(Error::Forbidden);
    }
    if proof.consumed_at.is_some()
        && proof.completion_credential_id.as_deref() != Some(actor.credential_id.as_str())
    {
        return Err(Error::Forbidden);
    }
    let scoped_actor = ActorContext {
        workspace_id: proof.workspace_id.clone().unwrap_or_default(),
        ..actor.clone()
    };
    authorize_account(&mut tx, actor).await?;
    if matches!(
        proof.action.as_str(),
        "ownership_accept" | "ownership_decline"
    ) || (proof.consumed_at.is_some() && proof.result.as_deref() == Some("accepted"))
    {
        crate::authorize_workspace(&mut tx, &scoped_actor).await?;
    }
    if !account_email(&mut tx, actor)
        .await?
        .eq_ignore_ascii_case(&proof.recipient_email)
    {
        return Err(Error::Forbidden);
    }
    if let Some(workspace_id) = proof.workspace_id.as_deref() {
        #[derive(sqlx::FromRow)]
        struct Workspace {
            workspace_id: String,
        }
        let sql = if pg {
            "SELECT workspace_id FROM workspaces WHERE workspace_id=$1 AND status='active' FOR UPDATE"
        } else {
            "SELECT workspace_id FROM workspaces WHERE workspace_id=$1 AND status='active'"
        };
        let workspace = krausen_core::dbtx_fetch_optional!(&mut tx, Workspace, sql, workspace_id)
            .map_err(db_error)?
            .ok_or(Error::Forbidden)?;
        if workspace.workspace_id != workspace_id {
            return Err(Error::Forbidden);
        }
        if proof.consumed_at.is_some() && proof.result.as_deref() == Some("ownership accepted") {
            #[derive(sqlx::FromRow)]
            struct Owner {
                owner_user_id: String,
            }
            let owner = krausen_core::dbtx_fetch_optional!(
                &mut tx,
                Owner,
                "SELECT owner_user_id FROM workspaces WHERE workspace_id=$1 AND status='active'",
                workspace_id
            )
            .map_err(db_error)?
            .ok_or(Error::Forbidden)?;
            if owner.owner_user_id != actor.actor_id {
                return Err(Error::Forbidden);
            }
        }
    }
    let digest = krausen_core::canonical_json::sha256(&serde_json::json!({"version":1,"proof_id":proof.proof_id,"action":proof.action,"target_id":proof.target_id,"workspace_id":proof.workspace_id,"expected_revision":proof.expected_revision})).map_err(|_| Error::Invalid("invalid proof identity"))?;
    let request = request_id.to_string();
    let key = RequestIdentity {
        actor: &scoped_actor,
        operation: "account.proof.complete.v1",
        request_id: &request,
        payload_digest: &digest,
    };
    if let Some(result) = recover_request(&mut tx, &key).await? {
        let result = serde_json::from_value(result)?;
        tx.commit().await.map_err(boundary_error)?;
        return Ok(result);
    }
    if proof.consumed_at.is_some()
        || proof.result.is_some()
        || proof.expires_at <= chrono::Utc::now().timestamp()
    {
        return Err(Error::Forbidden);
    }
    if proof.action == "discover" {
        if proof.target_id != actor.actor_id || proof.workspace_id.is_some() {
            return Err(Error::Forbidden);
        }
        let email = account_email(&mut tx, actor).await?;
        if !email.eq_ignore_ascii_case(&proof.recipient_email) {
            return Err(Error::Forbidden);
        }
        authorize_account(&mut tx, actor).await?;
        krausen_core::dbtx_execute!(&mut tx, "UPDATE account_action_proofs SET consumed_at=$1,result='verified',completion_credential_id=$2 WHERE proof_id=$3 AND consumed_at IS NULL", chrono::Utc::now().timestamp(), &actor.credential_id, proof_id).map_err(db_error)?;
        let account_actor = ActorContext {
            workspace_id: String::new(),
            ..actor.clone()
        };
        record_request(
            &mut tx,
            &key,
            &serde_json::to_value(InvitationProofOutcome::MailboxVerified)?,
            Some(proof_id),
        )
        .await?;
        record_change(
            &mut tx,
            &account_actor,
            "invitation.discovery.verified.v1",
            proof_id,
            0,
        )
        .await?;
        tx.commit().await.map_err(boundary_error)?;
        return Ok(InvitationProofOutcome::MailboxVerified);
    }
    if matches!(
        proof.action.as_str(),
        "ownership_accept" | "ownership_decline"
    ) {
        if !prepared_ownership {
            return Err(Error::Forbidden);
        }
        let result = crate::ownership::complete_in_tx(&mut tx, actor, &proof).await?;
        authorize_account(&mut tx, actor).await?;
        let credential = krausen_core::dbtx_fetch_optional!(
            &mut tx,
            Credential,
            credential_sql,
            &proof.credential_id,
            &actor.actor_id,
            chrono::Utc::now().timestamp()
        )
        .map_err(db_error)?
        .ok_or(Error::Forbidden)?;
        if credential.credential_id != proof.credential_id
            || proof.expires_at <= chrono::Utc::now().timestamp()
        {
            return Err(Error::Forbidden);
        }
        krausen_core::dbtx_execute!(&mut tx, "UPDATE account_action_proofs SET consumed_at=$1,result=$2,completion_credential_id=$3 WHERE proof_id=$4 AND consumed_at IS NULL", chrono::Utc::now().timestamp(), result.as_str(), &actor.credential_id, proof_id).map_err(db_error)?;
        record_request(
            &mut tx,
            &key,
            &serde_json::to_value(result)?,
            Some(&proof.target_id),
        )
        .await?;
        record_change(
            &mut tx,
            &scoped_actor,
            "workspace.ownership.response.complete.v1",
            &proof.target_id,
            proof.expected_revision.unwrap_or_default() + 1,
        )
        .await?;
        tx.commit().await.map_err(boundary_error)?;
        return Ok(result);
    }
    let invitation = eligible(&mut tx, actor, &proof.target_id).await?;
    if Some(invitation.workspace_id.as_str()) != proof.workspace_id.as_deref()
        || !invitation
            .email
            .eq_ignore_ascii_case(&proof.recipient_email)
    {
        return Err(Error::Forbidden);
    }
    // The resource lock may have blocked across the caller's expiry boundary.
    authorize_account(&mut tx, actor).await?;
    let credential = krausen_core::dbtx_fetch_optional!(
        &mut tx,
        Credential,
        credential_sql,
        &proof.credential_id,
        &actor.actor_id,
        chrono::Utc::now().timestamp()
    )
    .map_err(db_error)?
    .ok_or(Error::Forbidden)?;
    if credential.credential_id != proof.credential_id
        || proof.expires_at <= chrono::Utc::now().timestamp()
    {
        return Err(Error::Forbidden);
    }

    if !matches!(
        invitation.role.as_str(),
        "workspace_user" | "workspace_admin"
    ) {
        return Err(Error::Invalid("invalid invitation role"));
    }
    let result = match proof.action.as_str() {
        "accept" => {
            #[derive(sqlx::FromRow)]
            struct ExistingMembership {
                active: bool,
            }
            let existing = krausen_core::dbtx_fetch_optional!(
                &mut tx,
                ExistingMembership,
                "SELECT active FROM workspace_users WHERE workspace_id=$1 AND user_id=$2",
                &proof.workspace_id,
                &actor.actor_id
            )
            .map_err(db_error)?;
            if existing
                .as_ref()
                .is_some_and(|membership| membership.active)
            {
                return Err(Error::Conflict {
                    expected: 0,
                    actual: 1,
                });
            }
            if existing.is_some() {
                krausen_core::dbtx_execute!(&mut tx, "UPDATE workspace_users SET active=TRUE,role=$1 WHERE workspace_id=$2 AND user_id=$3 AND active=FALSE", &invitation.role, proof.workspace_id.as_deref(), &actor.actor_id).map_err(db_error)?;
            } else {
                krausen_core::dbtx_execute!(
                    &mut tx,
                    "INSERT INTO workspace_users (workspace_id,user_id,role) VALUES ($1,$2,$3)",
                    proof.workspace_id.as_deref(),
                    &actor.actor_id,
                    &invitation.role
                )
                .map_err(db_error)?;
            }
            krausen_core::dbtx_execute!(&mut tx, "UPDATE workspace_invitations SET status='accepted', accepted_at=CURRENT_TIMESTAMP, accepted_by_user_id=$1 WHERE invitation_id=$2 AND status='pending'", &actor.actor_id, &proof.target_id).map_err(db_error)?;
            InvitationProofOutcome::Accepted
        }
        "decline" => {
            krausen_core::dbtx_execute!(&mut tx, "UPDATE workspace_invitations SET status='declined' WHERE invitation_id=$1 AND status='pending'", &proof.target_id).map_err(db_error)?;
            InvitationProofOutcome::Declined
        }
        _ => return Err(Error::Forbidden),
    };
    krausen_core::dbtx_execute!(
        &mut tx,
        "UPDATE workspaces SET revision=revision+1 WHERE workspace_id=$1 AND status='active'",
        proof.workspace_id.as_deref()
    )
    .map_err(db_error)?;
    let now = chrono::Utc::now().timestamp();
    krausen_core::dbtx_execute!(&mut tx, "UPDATE account_action_proofs SET consumed_at=$1,result=$2,completion_credential_id=$3 WHERE proof_id=$4 AND consumed_at IS NULL", now, result.as_str(), &actor.credential_id, proof_id).map_err(db_error)?;
    let audit_actor = ActorContext {
        workspace_id: proof.workspace_id.unwrap_or_default(),
        ..actor.clone()
    };
    record_request(
        &mut tx,
        &key,
        &serde_json::to_value(result)?,
        Some(&proof.target_id),
    )
    .await?;
    record_change(
        &mut tx,
        &audit_actor,
        "invitation.response.v1",
        &proof.target_id,
        0,
    )
    .await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(result)
}

/// The initiating credential may inspect delivery and completion without receiving a token.
pub async fn status(
    pool: &DbPool,
    actor: &ActorContext,
    proof_id: &str,
) -> Result<krausen_types::account::InteractionStatus, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_account(&mut tx, actor).await?;
    let proof = krausen_core::dbtx_fetch_optional!(&mut tx, Proof, "SELECT * FROM account_action_proofs WHERE proof_id=$1 AND account_id=$2 AND credential_id=$3", proof_id, &actor.actor_id, &actor.credential_id).map_err(db_error)?.ok_or(Error::Forbidden)?;
    let state = if proof.consumed_at.is_some() {
        "completed"
    } else if proof.expires_at <= chrono::Utc::now().timestamp() {
        "expired"
    } else {
        let delivered: i64 = krausen_core::dbtx_fetch_scalar!(
            &mut tx,
            i64,
            "SELECT COUNT(*) FROM app_outbox WHERE outbox_id=$1 AND delivered_at IS NOT NULL",
            proof.outbox_id.as_deref()
        )
        .map_err(db_error)?;
        if delivered > 0 {
            "awaiting_recipient"
        } else {
            "delivery_pending"
        }
    };
    let result = krausen_types::account::InteractionStatus {
        interaction_id: proof.proof_id,
        state: state.into(),
        result: proof.result,
    };
    tx.commit().await.map_err(boundary_error)?;
    Ok(result)
}

async fn account_email(tx: &mut DbTx<'_>, actor: &ActorContext) -> Result<String, Error> {
    #[derive(sqlx::FromRow)]
    struct Email {
        email: String,
    }
    let row = krausen_core::dbtx_fetch_optional!(
        tx,
        Email,
        "SELECT email FROM users WHERE user_id=$1",
        &actor.actor_id
    )
    .map_err(db_error)?
    .ok_or(Error::Forbidden)?;
    Ok(row.email)
}

async fn verify_discovery(
    tx: &mut DbTx<'_>,
    actor: &ActorContext,
    proof_id: &str,
) -> Result<(), Error> {
    let pg = matches!(tx, DbTx::Postgres(_));
    let sql = if pg {
        "SELECT * FROM account_action_proofs WHERE proof_id=$1 AND account_id=$2 AND credential_id=$3 AND action='discover' FOR SHARE"
    } else {
        "SELECT * FROM account_action_proofs WHERE proof_id=$1 AND account_id=$2 AND credential_id=$3 AND action='discover'"
    };
    let proof = krausen_core::dbtx_fetch_optional!(
        tx,
        Proof,
        sql,
        proof_id,
        &actor.actor_id,
        &actor.credential_id
    )
    .map_err(db_error)?
    .ok_or(Error::Forbidden)?;
    let email = account_email(tx, actor).await?;
    if proof.expires_at <= chrono::Utc::now().timestamp()
        || proof.consumed_at.is_none()
        || proof.result.as_deref() != Some("verified")
        || proof.target_id != actor.actor_id
        || !proof.recipient_email.eq_ignore_ascii_case(&email)
    {
        return Err(Error::Forbidden);
    }
    Ok(())
}

/// Start a generic mailbox check without looking up or disclosing invitations.
pub async fn begin_discovery(
    pool: &DbPool,
    actor: &ActorContext,
    request_id: uuid::Uuid,
    mail_available: bool,
) -> Result<PendingInteraction, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_account(&mut tx, actor).await?;
    let email = account_email(&mut tx, actor).await?;
    let digest = format!(
        "{:x}",
        Sha256::digest(format!("invitation-discovery.v1\0{email}").as_bytes())
    );
    let request = request_id.to_string();
    let account_actor = ActorContext {
        workspace_id: String::new(),
        ..actor.clone()
    };
    let key = RequestIdentity {
        actor: &account_actor,
        operation: "invitation.discovery.begin.v1",
        request_id: &request,
        payload_digest: &digest,
    };
    if let Some(result) = recover_request(&mut tx, &key).await? {
        let result = serde_json::from_value(result)?;
        tx.commit().await.map_err(boundary_error)?;
        return Ok(result);
    }
    if !mail_available {
        return Err(Error::DeliveryUnavailable);
    }
    let proof_id = uuid::Uuid::new_v4().to_string();
    let expires_at = chrono::Utc::now().timestamp() + 900;
    krausen_core::dbtx_execute!(&mut tx, "INSERT INTO account_action_proofs (proof_id,account_id,credential_id,action,target_id,workspace_id,recipient_email,expires_at) VALUES ($1,$2,$3,'discover',$2,NULL,$4,$5)", &proof_id, &actor.actor_id, &actor.credential_id, &email, expires_at).map_err(db_error)?;
    let pending = PendingInteraction {
        interaction_id: proof_id.clone(),
        expires_at,
        next_action: "check_email".into(),
    };
    let outbox_id = enqueue_outbox(
        &mut tx,
        "",
        "account.invitation.proof.v1",
        &serde_json::json!({"proof_id":proof_id}),
    )
    .await?;
    krausen_core::dbtx_execute!(
        &mut tx,
        "UPDATE account_action_proofs SET outbox_id=$1 WHERE proof_id=$2",
        &outbox_id,
        &proof_id
    )
    .map_err(db_error)?;
    record_request(
        &mut tx,
        &key,
        &serde_json::to_value(&pending)?,
        Some(&proof_id),
    )
    .await?;
    record_change(
        &mut tx,
        &account_actor,
        "invitation.discovery.proof.pending.v1",
        &proof_id,
        0,
    )
    .await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(pending)
}

/// List invitations only after mailbox proof bound to this account and credential.
pub async fn list(
    pool: &DbPool,
    actor: &ActorContext,
    discovery_id: &str,
) -> Result<Vec<krausen_types::account::InvitationSummary>, Error> {
    #[derive(sqlx::FromRow)]
    struct Row {
        invitation_id: String,
        workspace_id: String,
        email: String,
        role: String,
        created_at: String,
        expires_at: String,
    }
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_account(&mut tx, actor).await?;
    verify_discovery(&mut tx, actor, discovery_id).await?;
    let email = account_email(&mut tx, actor).await?;
    let sql = if pool.is_postgres() {
        "SELECT invitation_id,workspace_id,email,role,CAST(created_at AS TEXT) AS created_at,CAST(expires_at AS TEXT) AS expires_at FROM workspace_invitations WHERE lower(email)=lower($1) AND status='pending' AND expires_at>CURRENT_TIMESTAMP ORDER BY created_at DESC LIMIT 100"
    } else {
        "SELECT invitation_id,workspace_id,email,role,CAST(created_at AS TEXT) AS created_at,CAST(expires_at AS TEXT) AS expires_at FROM workspace_invitations WHERE lower(email)=lower($1) AND status='pending' AND julianday(expires_at)>julianday('now') ORDER BY created_at DESC LIMIT 100"
    };
    let rows = match &mut tx {
        DbTx::Postgres(t) => {
            sqlx::query_as::<_, Row>(sql)
                .bind(&email)
                .fetch_all(&mut **t)
                .await
        }
        DbTx::Sqlite(t) => {
            sqlx::query_as::<_, Row>(sql)
                .bind(&email)
                .fetch_all(&mut **t)
                .await
        }
    }
    .map_err(db_error)?;
    authorize_account(&mut tx, actor).await?;
    verify_discovery(&mut tx, actor, discovery_id).await?;
    let result = rows
        .into_iter()
        .map(|r| krausen_types::account::InvitationSummary {
            invitation_id: r.invitation_id,
            workspace_id: r.workspace_id,
            email: r.email,
            role: r.role,
            created_at: r.created_at,
            expires_at: r.expires_at,
        })
        .collect();
    tx.commit().await.map_err(boundary_error)?;
    Ok(result)
}

/// Cancel a pending invitation under current administration and revision authority.
pub async fn cancel(
    pool: &DbPool,
    actor: &ActorContext,
    invitation_id: &str,
    expected: i64,
    request_id: uuid::Uuid,
) -> Result<i64, Error> {
    #[derive(sqlx::FromRow)]
    struct State {
        status: String,
    }
    #[derive(sqlx::FromRow)]
    struct Revision {
        revision: i64,
    }
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    if crate::authorize_workspace(&mut tx, actor).await? != "workspace_admin" {
        return Err(Error::Forbidden);
    }
    let pg = pool.is_postgres();
    let sql = if pg {
        "SELECT revision FROM workspaces WHERE workspace_id=$1 AND status='active' FOR UPDATE"
    } else {
        "SELECT revision FROM workspaces WHERE workspace_id=$1 AND status='active'"
    };
    let workspace = krausen_core::dbtx_fetch_optional!(&mut tx, Revision, sql, &actor.workspace_id)
        .map_err(db_error)?
        .ok_or(Error::Forbidden)?;
    if crate::authorize_workspace(&mut tx, actor).await? != "workspace_admin" {
        return Err(Error::Forbidden);
    }
    let digest = krausen_core::canonical_json::sha256(
        &serde_json::json!({"invitation_id":invitation_id,"expected_revision":expected}),
    )
    .map_err(|_| Error::Invalid("invitation identity"))?;
    let request = request_id.to_string();
    let key = RequestIdentity {
        actor,
        operation: "account.invitation.cancel.v1",
        request_id: &request,
        payload_digest: &digest,
    };
    if let Some(result) = recover_request(&mut tx, &key).await? {
        let revision = serde_json::from_value(result)?;
        tx.commit().await.map_err(boundary_error)?;
        return Ok(revision);
    }
    if workspace.revision != expected {
        return Err(Error::Conflict {
            expected,
            actual: workspace.revision,
        });
    }
    let sql = if pg {
        "SELECT status FROM workspace_invitations WHERE invitation_id=$1 AND workspace_id=$2 FOR UPDATE"
    } else {
        "SELECT status FROM workspace_invitations WHERE invitation_id=$1 AND workspace_id=$2"
    };
    let row =
        krausen_core::dbtx_fetch_optional!(&mut tx, State, sql, invitation_id, &actor.workspace_id)
            .map_err(db_error)?
            .ok_or(Error::Forbidden)?;
    if row.status != "pending" {
        return Err(Error::Invalid("invitation unavailable"));
    }
    let result = krausen_core::dbtx_execute!(&mut tx, "UPDATE workspace_invitations SET status='cancelled' WHERE invitation_id=$1 AND workspace_id=$2 AND status='pending'", invitation_id, &actor.workspace_id).map_err(db_error)?;
    if result.rows_affected() != 1 {
        return Err(Error::Retryable);
    }
    let revision = workspace.revision + 1;
    krausen_core::dbtx_execute!(
        &mut tx,
        "UPDATE workspaces SET revision=$1 WHERE workspace_id=$2 AND revision=$3",
        revision,
        &actor.workspace_id,
        workspace.revision
    )
    .map_err(db_error)?;
    record_request(
        &mut tx,
        &key,
        &serde_json::json!(revision),
        Some(invitation_id),
    )
    .await?;
    record_change(
        &mut tx,
        actor,
        "account.invitation.cancel.v1",
        invitation_id,
        revision,
    )
    .await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(revision)
}
