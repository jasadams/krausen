// SPDX-License-Identifier: AGPL-3.0-or-later
//! Real database tests for recipient, credential, action and retry binding.
use krausen_app::{
    ActorContext, Error,
    invitation::{self, InvitationAction, Proof},
};
use krausen_core::DbPool;

async fn exercise(db: DbPool) {
    let secret = b"test-only-installation-key";
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let owner = format!("owner-{suffix}");
    let recipient = format!("recipient-{suffix}");
    let other = format!("other-{suffix}");
    let workspace = format!("workspace-{suffix}");
    let foreign = format!("foreign-{suffix}");
    let email = format!("{suffix}@example.test");
    let invite = format!("invite-{suffix}");
    macro_rules! run { ($sql:expr $(, $arg:expr)*) => { match &db { DbPool::Postgres(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(), DbPool::Sqlite(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected() } }; }
    run!(
        "INSERT INTO users (user_id,email,active,verified) VALUES ($1,$2,TRUE,TRUE)",
        &owner,
        format!("owner-{suffix}@example.test")
    );
    // This verified flag models an operator-asserted email; it is not mailbox proof.
    run!(
        "INSERT INTO users (user_id,email,active,verified) VALUES ($1,$2,TRUE,TRUE)",
        &recipient,
        &email
    );
    run!(
        "INSERT INTO users (user_id,email,active,verified) VALUES ($1,$2,TRUE,TRUE)",
        &other,
        format!("other-{suffix}@example.test")
    );
    run!(
        "INSERT INTO workspaces (workspace_id,owner_user_id,status) VALUES ($1,$2,'active')",
        &workspace,
        &owner
    );
    run!(
        "INSERT INTO workspaces (workspace_id,owner_user_id,status) VALUES ($1,$2,'active')",
        &foreign,
        &other
    );
    run!(
        "INSERT INTO workspace_users (workspace_id,user_id,role,active) VALUES ($1,$2,'workspace_admin',TRUE)",
        &workspace,
        &owner
    );
    run!(
        "INSERT INTO workspace_users (workspace_id,user_id,role,active) VALUES ($1,$2,'workspace_user',TRUE)",
        &workspace,
        &other
    );
    run!(
        "INSERT INTO workspace_users (workspace_id,user_id,role,active) VALUES ($1,$2,'workspace_admin',TRUE)",
        &foreign,
        &other
    );
    let owner_credential = format!("owner-cred-{suffix}");
    let credential = format!("cred-{suffix}");
    let browser_credential = format!("browser-{suffix}");
    let other_credential = format!("other-cred-{suffix}");
    for (id, user) in [
        (&owner_credential, &owner),
        (&credential, &recipient),
        (&browser_credential, &recipient),
        (&other_credential, &other),
    ] {
        run!(
            "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ($1,$2,'browser','test','test',$3)",
            id,
            user,
            chrono::Utc::now().timestamp() + 3600
        );
    }
    let expiry = chrono::Utc::now() + chrono::Duration::days(1);
    run!(
        "INSERT INTO workspace_invitations (invitation_id,workspace_id,email,role,invited_by_user_id,expires_at) VALUES ($1,$2,$3,'workspace_user',$4,$5)",
        &invite,
        &workspace,
        &email,
        &owner,
        expiry
    );
    let actor = ActorContext {
        actor_id: recipient.clone(),
        workspace_id: workspace.clone(),
        credential_id: credential.clone(),
        access_expires_at: chrono::Utc::now().timestamp() + 3600,
    };
    let browser = ActorContext {
        credential_id: browser_credential,
        ..actor.clone()
    };
    let wrong = ActorContext {
        actor_id: other,
        credential_id: other_credential,
        ..actor.clone()
    };
    let owner_actor = ActorContext {
        actor_id: owner.clone(),
        workspace_id: workspace.clone(),
        credential_id: owner_credential.clone(),
        access_expires_at: chrono::Utc::now().timestamp() + 3600,
    };
    let team = krausen_app::team::members(&db, &owner_actor).await.unwrap();
    let promote = krausen_types::account::TeamChange::SetRole {
        user_id: wrong.actor_id.clone(),
        role: "workspace_admin".into(),
    };
    let team_request = uuid::Uuid::new_v4();
    let team_revision = krausen_app::team::change(
        &db,
        &owner_actor,
        promote.clone(),
        team.revision,
        team_request,
    )
    .await
    .unwrap();
    assert_eq!(
        team_revision,
        krausen_app::team::change(&db, &owner_actor, promote, team.revision, team_request)
            .await
            .unwrap()
    );
    assert!(matches!(
        krausen_app::team::change(
            &db,
            &owner_actor,
            krausen_types::account::TeamChange::Remove {
                user_id: owner.clone()
            },
            team_revision,
            uuid::Uuid::new_v4()
        )
        .await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        krausen_app::team::change(
            &db,
            &owner_actor,
            krausen_types::account::TeamChange::Remove {
                user_id: wrong.actor_id.clone()
            },
            team.revision,
            uuid::Uuid::new_v4()
        )
        .await,
        Err(Error::Conflict { .. })
    ));
    krausen_app::team::change(
        &db,
        &owner_actor,
        krausen_types::account::TeamChange::SetRole {
            user_id: wrong.actor_id.clone(),
            role: "workspace_user".into(),
        },
        team_revision,
        uuid::Uuid::new_v4(),
    )
    .await
    .unwrap();
    let invitation_revision = krausen_app::team::members(&db, &owner_actor)
        .await
        .unwrap()
        .revision;
    let invited_email = format!("new-{suffix}@example.test");
    let create_id = uuid::Uuid::new_v4();
    assert!(matches!(
        krausen_app::team::invite(
            &db,
            &owner_actor,
            &invited_email,
            "workspace_user",
            invitation_revision,
            create_id,
            false
        )
        .await,
        Err(Error::DeliveryUnavailable)
    ));
    let queued = krausen_app::team::invite(
        &db,
        &owner_actor,
        &invited_email,
        "workspace_user",
        invitation_revision,
        create_id,
        true,
    )
    .await
    .unwrap();
    assert_eq!(queued.delivery, "pending");
    assert!(
        krausen_app::team::invitations(&db, &owner_actor)
            .await
            .unwrap()
            .iter()
            .any(|item| item.invitation_id == queued.invitation_id)
    );
    assert!(matches!(
        krausen_app::team::invitations(&db, &wrong).await,
        Err(Error::Forbidden)
    ));
    assert_eq!(
        queued,
        krausen_app::team::invite(
            &db,
            &owner_actor,
            &invited_email,
            "workspace_user",
            invitation_revision,
            create_id,
            false
        )
        .await
        .unwrap()
    );
    let sent = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let failed = sent.clone();
    krausen_server::invitation_delivery::deliver_invitations_with(
        &db,
        "https://example.test",
        move |email, url| {
            let sent = failed.clone();
            async move {
                sent.lock().unwrap().push((email, url));
                false
            }
        },
    )
    .await
    .unwrap();
    let retry = sent.clone();
    krausen_server::invitation_delivery::deliver_invitations_with(
        &db,
        "https://example.test",
        move |email, url| {
            let sent = retry.clone();
            async move {
                sent.lock().unwrap().push((email, url));
                true
            }
        },
    )
    .await
    .unwrap();
    let sent = sent.lock().unwrap();
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0], sent[1]);
    assert_eq!(sent[0].0, invited_email);
    drop(sent);
    run!(
        "UPDATE users SET extra_metadata = '{\"other\":\"kept\"}' WHERE user_id=$1",
        &owner
    );
    let initial_profile = krausen_app::account::profile(&db, &owner_actor)
        .await
        .unwrap();
    let profile_request = uuid::Uuid::new_v4();
    let change = krausen_types::account::ProfileChange::Theme {
        theme: "dark".into(),
    };
    let updated = krausen_app::account::save(
        &db,
        &owner_actor,
        change.clone(),
        initial_profile.revision,
        profile_request,
    )
    .await
    .unwrap();
    assert_eq!(updated.theme, "dark");
    assert_eq!(
        updated,
        krausen_app::account::save(
            &db,
            &owner_actor,
            change,
            initial_profile.revision,
            profile_request
        )
        .await
        .unwrap()
    );
    assert!(matches!(
        krausen_app::account::save(
            &db,
            &owner_actor,
            krausen_types::account::ProfileChange::Name {
                name: "Changed".into()
            },
            initial_profile.revision,
            uuid::Uuid::new_v4()
        )
        .await,
        Err(Error::Conflict { .. })
    ));
    assert!(matches!(
        krausen_app::account::save(
            &db,
            &owner_actor,
            krausen_types::account::ProfileChange::Name {
                name: "Changed".into()
            },
            initial_profile.revision,
            profile_request
        )
        .await,
        Err(Error::RequestIdentityConflict)
    ));
    let metadata: String = match &db {
        DbPool::Postgres(p) => {
            sqlx::query_scalar("SELECT CAST(extra_metadata AS TEXT) FROM users WHERE user_id=$1")
                .bind(&owner)
                .fetch_one(p)
                .await
                .unwrap()
        }
        DbPool::Sqlite(p) => {
            sqlx::query_scalar("SELECT CAST(extra_metadata AS TEXT) FROM users WHERE user_id=$1")
                .bind(&owner)
                .fetch_one(p)
                .await
                .unwrap()
        }
    };
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&metadata).unwrap()["other"],
        "kept"
    );
    let selection_request = uuid::Uuid::new_v4();
    let selected = krausen_app::account::select_workspace(&db, &wrong, &foreign, selection_request)
        .await
        .unwrap();
    assert_eq!(selected.workspace_id, foreign);
    assert_eq!(
        selected,
        krausen_app::account::select_workspace(&db, &wrong, &foreign, selection_request)
            .await
            .unwrap()
    );
    assert_eq!(
        krausen_app::account::workspaces(&db, &wrong)
            .await
            .unwrap()
            .len(),
        2
    );
    assert!(matches!(
        krausen_app::account::select_workspace(&db, &owner_actor, &foreign, uuid::Uuid::new_v4())
            .await,
        Err(Error::Forbidden)
    ));
    run!(
        "UPDATE workspace_users SET active=FALSE WHERE workspace_id=$1 AND user_id=$2",
        &foreign,
        &wrong.actor_id
    );
    assert!(matches!(
        krausen_app::account::select_workspace(&db, &wrong, &foreign, selection_request).await,
        Err(Error::Forbidden)
    ));
    run!(
        "UPDATE workspace_users SET active=TRUE WHERE workspace_id=$1 AND user_id=$2",
        &foreign,
        &wrong.actor_id
    );
    assert!(matches!(
        invitation::begin_discovery(&db, &actor, uuid::Uuid::new_v4(), false).await,
        Err(Error::DeliveryUnavailable)
    ));
    let request = uuid::Uuid::new_v4();
    let pending = invitation::begin_discovery(&db, &actor, request, true)
        .await
        .unwrap();
    assert_eq!(
        pending,
        invitation::begin_discovery(&db, &actor, request, false)
            .await
            .unwrap()
    );
    assert_eq!(
        invitation::status(&db, &actor, &pending.interaction_id)
            .await
            .unwrap()
            .state,
        "delivery_pending"
    );
    let captured = std::sync::Arc::new(std::sync::Mutex::new(Vec::<(String, String)>::new()));
    let failures = captured.clone();
    krausen_server::invitation_delivery::deliver_pending_with(
        &db,
        "https://example.test",
        secret,
        move |email, url| {
            let captured = failures.clone();
            async move {
                captured.lock().unwrap().push((email, url));
                false
            }
        },
    )
    .await
    .unwrap();
    assert_eq!(
        invitation::status(&db, &actor, &pending.interaction_id)
            .await
            .unwrap()
            .state,
        "delivery_pending"
    );
    let successes = captured.clone();
    krausen_server::invitation_delivery::deliver_pending_with(
        &db,
        "https://example.test",
        secret,
        move |email, url| {
            let captured = successes.clone();
            async move {
                captured.lock().unwrap().push((email, url));
                true
            }
        },
    )
    .await
    .unwrap();
    assert_eq!(
        invitation::status(&db, &actor, &pending.interaction_id)
            .await
            .unwrap()
            .state,
        "awaiting_recipient"
    );
    let delivered: Vec<_> = captured
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, url)| url.contains(&pending.interaction_id))
        .cloned()
        .collect();
    assert_eq!(delivered.len(), 2);
    assert_eq!(delivered[0], delivered[1]);
    assert_eq!(delivered[0].0, email);
    let stored = match &db {
        DbPool::Postgres(p) => sqlx::query_scalar::<_, String>(
            "SELECT payload_json FROM app_outbox WHERE outbox_id=$1",
        )
        .bind(&proof_outbox_id(&db, &pending.interaction_id).await)
        .fetch_one(p)
        .await
        .unwrap(),
        DbPool::Sqlite(p) => sqlx::query_scalar::<_, String>(
            "SELECT payload_json FROM app_outbox WHERE outbox_id=$1",
        )
        .bind(&proof_outbox_id(&db, &pending.interaction_id).await)
        .fetch_one(p)
        .await
        .unwrap(),
    };
    assert!(
        !delivered[0]
            .1
            .split("code=")
            .nth(1)
            .is_some_and(|code| stored.contains(code))
    );
    assert!(
        invitation::list(&db, &actor, &pending.interaction_id)
            .await
            .is_err()
    );
    assert!(
        invitation::begin(
            &db,
            &actor,
            &invite,
            &pending.interaction_id,
            InvitationAction::Accept,
            uuid::Uuid::new_v4(),
            true
        )
        .await
        .is_err()
    );
    let proof: Proof = krausen_core::db_fetch_optional!(
        &db,
        Proof,
        "SELECT * FROM account_action_proofs WHERE proof_id=$1",
        &pending.interaction_id
    )
    .unwrap()
    .unwrap();
    let code = invitation::delivery_token(secret, &proof).unwrap();
    assert!(
        invitation::complete(
            &db,
            &wrong,
            &pending.interaction_id,
            &code,
            uuid::Uuid::parse_str(&pending.interaction_id).unwrap(),
            secret
        )
        .await
        .is_err()
    );
    assert!(
        invitation::complete(
            &db,
            &browser,
            &pending.interaction_id,
            "wrong",
            uuid::Uuid::parse_str(&pending.interaction_id).unwrap(),
            secret
        )
        .await
        .is_err()
    );
    assert_eq!(
        invitation::complete(
            &db,
            &browser,
            &pending.interaction_id,
            &code,
            uuid::Uuid::parse_str(&pending.interaction_id).unwrap(),
            secret
        )
        .await
        .unwrap(),
        krausen_types::account::InvitationProofOutcome::MailboxVerified
    );
    assert_eq!(
        invitation::complete(
            &db,
            &browser,
            &pending.interaction_id,
            &code,
            uuid::Uuid::parse_str(&pending.interaction_id).unwrap(),
            secret
        )
        .await
        .unwrap(),
        krausen_types::account::InvitationProofOutcome::MailboxVerified
    );
    assert!(
        invitation::complete(
            &db,
            &browser,
            &pending.interaction_id,
            &code,
            uuid::Uuid::new_v4(),
            secret
        )
        .await
        .is_err()
    );
    assert!(
        invitation::complete(
            &db,
            &actor,
            &pending.interaction_id,
            &code,
            uuid::Uuid::parse_str(&pending.interaction_id).unwrap(),
            secret
        )
        .await
        .is_err()
    );
    assert!(
        invitation::list(&db, &wrong, &pending.interaction_id)
            .await
            .is_err()
    );
    let list = invitation::list(&db, &actor, &pending.interaction_id)
        .await
        .unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].invitation_id, invite);
    // A failure after the membership write must roll back the membership,
    // invitation state and consumed proof together on either database.
    let rollback_workspace = format!("rollback-{suffix}");
    let rollback_invite = format!("rollback-inv-{suffix}");
    run!(
        "INSERT INTO workspaces (workspace_id,owner_user_id,status) VALUES ($1,$2,'active')",
        &rollback_workspace,
        &owner
    );
    run!(
        "INSERT INTO workspace_users (workspace_id,user_id,role,active) VALUES ($1,$2,'workspace_admin',TRUE)",
        &rollback_workspace,
        &owner
    );
    run!(
        "INSERT INTO workspace_invitations (invitation_id,workspace_id,email,role,invited_by_user_id,expires_at) VALUES ($1,$2,$3,'workspace_user',$4,$5)",
        &rollback_invite,
        &rollback_workspace,
        &email,
        &owner,
        expiry
    );
    let rollback_actor = ActorContext {
        workspace_id: rollback_workspace.clone(),
        ..actor.clone()
    };
    let rollback = invitation::begin(
        &db,
        &rollback_actor,
        &rollback_invite,
        &pending.interaction_id,
        InvitationAction::Accept,
        uuid::Uuid::new_v4(),
        true,
    )
    .await
    .unwrap();
    let rollback_proof: Proof = krausen_core::db_fetch_optional!(
        &db,
        Proof,
        "SELECT * FROM account_action_proofs WHERE proof_id=$1",
        &rollback.interaction_id
    )
    .unwrap()
    .unwrap();
    let rollback_code = invitation::delivery_token(secret, &rollback_proof).unwrap();
    let trigger = format!("krausen_fail_{suffix}");
    if db.is_postgres() {
        run!(&format!(
            "CREATE FUNCTION {trigger}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected rollback'; END $$"
        ));
        run!(&format!(
            "CREATE TRIGGER {trigger} BEFORE INSERT ON app_changes FOR EACH ROW WHEN (NEW.operation = 'invitation.response.v1') EXECUTE FUNCTION {trigger}()"
        ));
    } else {
        run!(&format!(
            "CREATE TRIGGER {trigger} BEFORE INSERT ON app_changes WHEN NEW.operation = 'invitation.response.v1' BEGIN SELECT RAISE(ABORT, 'injected rollback'); END"
        ));
    }
    assert!(
        invitation::complete(
            &db,
            &browser,
            &rollback.interaction_id,
            &rollback_code,
            uuid::Uuid::parse_str(&rollback.interaction_id).unwrap(),
            secret
        )
        .await
        .is_err()
    );
    let proof_after: Proof = krausen_core::db_fetch_optional!(
        &db,
        Proof,
        "SELECT * FROM account_action_proofs WHERE proof_id=$1",
        &rollback.interaction_id
    )
    .unwrap()
    .unwrap();
    assert!(proof_after.consumed_at.is_none());
    let member_count: i64 = krausen_core::db_fetch_scalar!(
        &db,
        i64,
        "SELECT COUNT(*) FROM workspace_users WHERE workspace_id=$1 AND user_id=$2",
        &rollback_workspace,
        &recipient
    )
    .unwrap();
    assert_eq!(member_count, 0);
    if db.is_postgres() {
        run!(&format!("DROP TRIGGER {trigger} ON app_changes"));
        run!(&format!("DROP FUNCTION {trigger}()"));
    } else {
        run!(&format!("DROP TRIGGER {trigger}"));
    }
    assert_eq!(
        invitation::complete(
            &db,
            &browser,
            &rollback.interaction_id,
            &rollback_code,
            uuid::Uuid::parse_str(&rollback.interaction_id).unwrap(),
            secret
        )
        .await
        .unwrap(),
        krausen_types::account::InvitationProofOutcome::Accepted
    );
    // A committed completion is recovered after first-use proof expiry; a
    // retained tombstone prevents reexecution when its result has expired.
    let lost = invitation::begin_discovery(&db, &actor, uuid::Uuid::new_v4(), true)
        .await
        .unwrap();
    run!(
        "UPDATE account_action_proofs SET expires_at=$1 WHERE proof_id=$2",
        chrono::Utc::now().timestamp() + 2,
        &lost.interaction_id
    );
    let lost_row: Proof = krausen_core::db_fetch_optional!(
        &db,
        Proof,
        "SELECT * FROM account_action_proofs WHERE proof_id=$1",
        &lost.interaction_id
    )
    .unwrap()
    .unwrap();
    let lost_code = invitation::delivery_token(secret, &lost_row).unwrap();
    let completion_id = uuid::Uuid::new_v4();
    assert_eq!(
        invitation::complete(
            &db,
            &browser,
            &lost.interaction_id,
            &lost_code,
            completion_id,
            secret
        )
        .await
        .unwrap(),
        krausen_types::account::InvitationProofOutcome::MailboxVerified
    );
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    assert_eq!(
        invitation::complete(
            &db,
            &browser,
            &lost.interaction_id,
            &lost_code,
            completion_id,
            secret
        )
        .await
        .unwrap(),
        krausen_types::account::InvitationProofOutcome::MailboxVerified
    );
    run!(
        "UPDATE auth_credentials SET revoked_at=$1 WHERE credential_id=$2",
        chrono::Utc::now().timestamp(),
        &credential
    );
    assert!(
        invitation::complete(
            &db,
            &browser,
            &lost.interaction_id,
            &lost_code,
            completion_id,
            secret
        )
        .await
        .is_err()
    );
    run!(
        "UPDATE auth_credentials SET revoked_at=NULL WHERE credential_id=$1",
        &credential
    );
    run!(
        "UPDATE auth_credentials SET revoked_at=$1 WHERE credential_id=$2",
        chrono::Utc::now().timestamp(),
        &browser.credential_id
    );
    assert!(
        invitation::complete(
            &db,
            &browser,
            &lost.interaction_id,
            &lost_code,
            completion_id,
            secret
        )
        .await
        .is_err()
    );
    run!(
        "UPDATE auth_credentials SET revoked_at=NULL WHERE credential_id=$1",
        &browser.credential_id
    );
    let other_proof = invitation::begin_discovery(&db, &actor, uuid::Uuid::new_v4(), true)
        .await
        .unwrap();
    let other_row: Proof = krausen_core::db_fetch_optional!(
        &db,
        Proof,
        "SELECT * FROM account_action_proofs WHERE proof_id=$1",
        &other_proof.interaction_id
    )
    .unwrap()
    .unwrap();
    let other_code = invitation::delivery_token(secret, &other_row).unwrap();
    assert!(matches!(
        invitation::complete(
            &db,
            &browser,
            &other_proof.interaction_id,
            &other_code,
            completion_id,
            secret
        )
        .await,
        Err(Error::RequestIdentityConflict)
    ));
    run!(
        "UPDATE app_requests SET result_expires_at=$1,result_json=NULL WHERE actor_id=$2 AND workspace_id='' AND operation='account.proof.complete.v1' AND request_id=$3",
        chrono::Utc::now().timestamp() - 1,
        &recipient,
        completion_id.to_string()
    );
    assert!(matches!(
        invitation::complete(
            &db,
            &browser,
            &lost.interaction_id,
            &lost_code,
            completion_id,
            secret
        )
        .await,
        Err(Error::ResultExpired { .. })
    ));
    let expired_invite = format!("expired-invite-{suffix}");
    run!(
        "INSERT INTO workspace_invitations (invitation_id,workspace_id,email,role,invited_by_user_id,expires_at) VALUES ($1,$2,$3,'workspace_user',$4,$5)",
        &expired_invite,
        &workspace,
        &email,
        &owner,
        chrono::Utc::now() - chrono::Duration::days(1)
    );
    assert!(
        invitation::begin(
            &db,
            &actor,
            &expired_invite,
            &pending.interaction_id,
            InvitationAction::Accept,
            uuid::Uuid::new_v4(),
            true
        )
        .await
        .is_err()
    );
    let expired = invitation::begin(
        &db,
        &actor,
        &invite,
        &pending.interaction_id,
        InvitationAction::Accept,
        uuid::Uuid::new_v4(),
        true,
    )
    .await
    .unwrap();
    run!(
        "UPDATE account_action_proofs SET expires_at=$1 WHERE proof_id=$2",
        chrono::Utc::now().timestamp() - 1,
        &expired.interaction_id
    );
    let expired_row: Proof = krausen_core::db_fetch_optional!(
        &db,
        Proof,
        "SELECT * FROM account_action_proofs WHERE proof_id=$1",
        &expired.interaction_id
    )
    .unwrap()
    .unwrap();
    let expired_code = invitation::delivery_token(secret, &expired_row).unwrap();
    assert!(
        invitation::complete(
            &db,
            &browser,
            &expired.interaction_id,
            &expired_code,
            uuid::Uuid::parse_str(&expired.interaction_id).unwrap(),
            secret
        )
        .await
        .is_err()
    );
    let accept = invitation::begin(
        &db,
        &actor,
        &invite,
        &pending.interaction_id,
        InvitationAction::Accept,
        uuid::Uuid::new_v4(),
        true,
    )
    .await
    .unwrap();
    let decline = invitation::begin(
        &db,
        &actor,
        &invite,
        &pending.interaction_id,
        InvitationAction::Decline,
        uuid::Uuid::new_v4(),
        true,
    )
    .await
    .unwrap();
    let a: Proof = krausen_core::db_fetch_optional!(
        &db,
        Proof,
        "SELECT * FROM account_action_proofs WHERE proof_id=$1",
        &accept.interaction_id
    )
    .unwrap()
    .unwrap();
    let d: Proof = krausen_core::db_fetch_optional!(
        &db,
        Proof,
        "SELECT * FROM account_action_proofs WHERE proof_id=$1",
        &decline.interaction_id
    )
    .unwrap()
    .unwrap();
    let acode = invitation::delivery_token(secret, &a).unwrap();
    let dcode = invitation::delivery_token(secret, &d).unwrap();
    assert_eq!(
        invitation::preview(&db, &browser, &accept.interaction_id, &acode, secret)
            .await
            .unwrap()
            .action,
        "accept"
    );
    assert_eq!(
        invitation::preview(&db, &browser, &decline.interaction_id, &dcode, secret)
            .await
            .unwrap()
            .action,
        "decline"
    );
    assert!(
        invitation::preview(&db, &wrong, &accept.interaction_id, &acode, secret)
            .await
            .is_err()
    );
    assert!(
        invitation::preview(&db, &browser, &accept.interaction_id, &dcode, secret)
            .await
            .is_err()
    );
    assert!(
        invitation::complete(
            &db,
            &browser,
            &accept.interaction_id,
            &dcode,
            uuid::Uuid::parse_str(&accept.interaction_id).unwrap(),
            secret
        )
        .await
        .is_err()
    );
    run!(
        "UPDATE auth_credentials SET revoked_at=$1 WHERE credential_id=$2",
        chrono::Utc::now().timestamp(),
        &credential
    );
    assert!(
        invitation::complete(
            &db,
            &browser,
            &accept.interaction_id,
            &acode,
            uuid::Uuid::parse_str(&accept.interaction_id).unwrap(),
            secret
        )
        .await
        .is_err()
    );
    run!(
        "UPDATE auth_credentials SET revoked_at=NULL WHERE credential_id=$1",
        &credential
    );
    let (one, two) = tokio::join!(
        invitation::complete(
            &db,
            &browser,
            &accept.interaction_id,
            &acode,
            uuid::Uuid::parse_str(&accept.interaction_id).unwrap(),
            secret
        ),
        invitation::complete(
            &db,
            &browser,
            &decline.interaction_id,
            &dcode,
            uuid::Uuid::parse_str(&decline.interaction_id).unwrap(),
            secret
        )
    );
    assert_ne!(one.is_ok(), two.is_ok());
    // Cancellation competes with confirmation under the same invitation lock.
    let race_workspace = format!("race-{suffix}");
    let race_invite = format!("race-inv-{suffix}");
    run!(
        "INSERT INTO workspaces (workspace_id,owner_user_id,status) VALUES ($1,$2,'active')",
        &race_workspace,
        &owner
    );
    run!(
        "INSERT INTO workspace_users (workspace_id,user_id,role,active) VALUES ($1,$2,'workspace_admin',TRUE)",
        &race_workspace,
        &owner
    );
    run!(
        "INSERT INTO workspace_invitations (invitation_id,workspace_id,email,role,invited_by_user_id,expires_at) VALUES ($1,$2,$3,'workspace_user',$4,$5)",
        &race_invite,
        &race_workspace,
        &email,
        &owner,
        expiry
    );
    let race_recipient = ActorContext {
        workspace_id: race_workspace.clone(),
        ..actor.clone()
    };
    let race_owner = ActorContext {
        actor_id: owner.clone(),
        workspace_id: race_workspace,
        credential_id: owner_credential,
        access_expires_at: chrono::Utc::now().timestamp() + 3600,
    };
    let race_proof = invitation::begin(
        &db,
        &race_recipient,
        &race_invite,
        &pending.interaction_id,
        InvitationAction::Accept,
        uuid::Uuid::new_v4(),
        true,
    )
    .await
    .unwrap();
    let row: Proof = krausen_core::db_fetch_optional!(
        &db,
        Proof,
        "SELECT * FROM account_action_proofs WHERE proof_id=$1",
        &race_proof.interaction_id
    )
    .unwrap()
    .unwrap();
    let code = invitation::delivery_token(secret, &row).unwrap();
    let race_revision = krausen_app::team::members(&db, &race_owner)
        .await
        .unwrap()
        .revision;
    let (accepted, cancelled) = tokio::join!(
        invitation::complete(
            &db,
            &browser,
            &race_proof.interaction_id,
            &code,
            uuid::Uuid::parse_str(&race_proof.interaction_id).unwrap(),
            secret
        ),
        invitation::cancel(
            &db,
            &race_owner,
            &race_invite,
            race_revision,
            uuid::Uuid::new_v4()
        )
    );
    assert_ne!(accepted.is_ok(), cancelled.is_ok());

    assert!(
        invitation::complete(
            &db,
            &browser,
            &accept.interaction_id,
            &acode,
            uuid::Uuid::new_v4(),
            secret
        )
        .await
        .is_err()
    );
    let transfer_revision = krausen_app::team::members(&db, &owner_actor)
        .await
        .unwrap()
        .revision;
    let transfer_request = uuid::Uuid::new_v4();
    assert!(matches!(
        krausen_app::ownership::start(
            &db,
            &owner_actor,
            &wrong.actor_id,
            transfer_revision,
            transfer_request,
            false
        )
        .await,
        Err(Error::DeliveryUnavailable)
    ));
    let transfer = krausen_app::ownership::start(
        &db,
        &owner_actor,
        &wrong.actor_id,
        transfer_revision,
        transfer_request,
        true,
    )
    .await
    .unwrap();
    assert_eq!(
        transfer,
        krausen_app::ownership::start(
            &db,
            &owner_actor,
            &wrong.actor_id,
            transfer_revision,
            transfer_request,
            false
        )
        .await
        .unwrap()
    );
    assert!(
        krausen_app::ownership::get(&db, &wrong, &transfer.transfer_id)
            .await
            .is_ok()
    );
    assert!(
        krausen_app::ownership::get(&db, &actor, &transfer.transfer_id)
            .await
            .is_err()
    );
    assert!(
        krausen_app::ownership::begin_response(
            &db,
            &actor,
            &transfer.transfer_id,
            true,
            transfer.revision,
            uuid::Uuid::new_v4(),
            true
        )
        .await
        .is_err()
    );
    let action_id = uuid::Uuid::new_v4();
    let action = krausen_app::ownership::begin_response(
        &db,
        &wrong,
        &transfer.transfer_id,
        true,
        transfer.revision,
        action_id,
        true,
    )
    .await
    .unwrap();
    assert_eq!(
        action,
        krausen_app::ownership::begin_response(
            &db,
            &wrong,
            &transfer.transfer_id,
            true,
            transfer.revision,
            action_id,
            false
        )
        .await
        .unwrap()
    );
    let transfer_proof: Proof = krausen_core::db_fetch_optional!(
        &db,
        Proof,
        "SELECT * FROM account_action_proofs WHERE proof_id=$1",
        &action.interaction_id
    )
    .unwrap()
    .unwrap();
    let transfer_code = invitation::delivery_token(secret, &transfer_proof).unwrap();
    assert_eq!(
        invitation::preview(&db, &wrong, &action.interaction_id, &transfer_code, secret)
            .await
            .unwrap()
            .action,
        "ownership_accept"
    );
    assert!(
        invitation::complete(
            &db,
            &actor,
            &action.interaction_id,
            &transfer_code,
            uuid::Uuid::new_v4(),
            secret
        )
        .await
        .is_err()
    );
    let completion_id = uuid::Uuid::new_v4();
    let completed = invitation::complete(
        &db,
        &wrong,
        &action.interaction_id,
        &transfer_code,
        completion_id,
        secret,
    )
    .await
    .unwrap();
    assert_eq!(
        completed,
        krausen_types::account::InvitationProofOutcome::OwnershipAccepted
    );
    assert_eq!(
        completed,
        invitation::complete(
            &db,
            &wrong,
            &action.interaction_id,
            &transfer_code,
            completion_id,
            secret
        )
        .await
        .unwrap()
    );
    let context = krausen_app::account::context(&db, &wrong).await.unwrap();
    assert!(context.is_owner);
    assert_eq!(context.workspace.unwrap().role, "workspace_admin");
}
#[tokio::test]
async fn sqlite_recipient_proofs() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations-sqlite")
        .run(&pool)
        .await
        .unwrap();
    exercise(DbPool::Sqlite(pool)).await;
}
#[tokio::test]
async fn postgres_recipient_proofs() {
    let url =
        std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL URL required");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    exercise(DbPool::Postgres(pool)).await;
}

async fn proof_outbox_id(db: &DbPool, proof_id: &str) -> String {
    match db {
        DbPool::Postgres(p) => {
            sqlx::query_scalar("SELECT outbox_id FROM account_action_proofs WHERE proof_id=$1")
                .bind(proof_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
        DbPool::Sqlite(p) => {
            sqlx::query_scalar("SELECT outbox_id FROM account_action_proofs WHERE proof_id=$1")
                .bind(proof_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
    }
}
