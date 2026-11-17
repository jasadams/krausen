// SPDX-License-Identifier: AGPL-3.0-or-later

//! Invitation acceptance and workspace switching, ported from Kyomi's
//! `session.rs` and `workspace_service.rs` test modules and run on SQLite and
//! PostgreSQL through the shared `signup_tests` harness.

use krausen_core::DbPool;

use super::*;
use crate::auth_service::signup_tests::{both_backends, email, postgres_db, sqlite_db};
use crate::session::switch_active_workspace;
use crate::token_service::DeviceInfo;

fn device() -> DeviceInfo {
    DeviceInfo {
        user_agent: None,
        ip_address: None,
        country_code: None,
        oauth_client_id: None,
    }
}

struct Fixture {
    user_id: String,
    user_email: String,
    home_workspace_id: String,
    joined_workspace_id: String,
    invitation_id: String,
}

/// A user with a home workspace, and a pending invitation into another
/// workspace owned by someone else.
async fn fixture(db: &DbPool) -> Fixture {
    let user_email = email("invitee");
    let user = crate::user_service::create_user(db, &user_email, Some("Invitee"), true)
        .await
        .expect("create invitee");
    let home_workspace_id =
        crate::user_service::create_workspace_for_user(db, &user.user_id, Some("Invitee"), &user_email, None)
            .await
            .expect("create home workspace");

    let owner_email = email("owner");
    let owner = crate::user_service::create_user(db, &owner_email, Some("Owner"), true)
        .await
        .expect("create owner");
    let joined_workspace_id =
        crate::user_service::create_workspace_for_user(db, &owner.user_id, Some("Owner"), &owner_email, None)
            .await
            .expect("create joined workspace");

    let invitation_id = format!("inv-{}", uuid::Uuid::new_v4());
    create_invitation(
        db,
        &invitation_id,
        &joined_workspace_id,
        &user_email,
        "workspace_user",
        &owner.user_id,
        chrono::Utc::now() + chrono::Duration::days(1),
    )
    .await
    .expect("create invitation");

    Fixture {
        user_id: user.user_id,
        user_email,
        home_workspace_id,
        joined_workspace_id,
        invitation_id,
    }
}

async fn user_of(db: &DbPool, user_id: &str) -> krausen_core::models::User {
    crate::user_service::get_user_by_id(db, user_id)
        .await
        .expect("query user")
        .expect("user exists")
}

async fn kv() -> krausen_core::KVPool {
    krausen_core::kv_store_memory::InMemoryKVStore::new_pool()
}

/// A user cannot switch into a workspace they have no active membership in.
async fn switch_rejects_workspace_with_no_active_membership(db: &DbPool) {
    let f = fixture(db).await;
    let user = user_of(db, &f.user_id).await;

    let result = switch_active_workspace(
        db,
        &kv().await,
        "test-secret",
        &user,
        &f.joined_workspace_id,
        &device(),
    )
    .await;

    match result {
        Err(krausen_core::Error::Forbidden(msg)) => {
            assert!(msg.contains("do not have access"), "message: {msg}")
        }
        Err(other) => panic!("expected Forbidden error, got: {other:?}"),
        Ok(_) => panic!("expected Forbidden error, but switch succeeded"),
    }
}

/// Accepting an invitation makes the joined workspace switchable-into, and
/// switching persists it as the user's active workspace.
async fn accept_then_switch_drops_user_into_joined_workspace(db: &DbPool) {
    let f = fixture(db).await;
    accept_invitation_for_user(db, &f.invitation_id, &f.user_id)
        .await
        .expect("accept invitation");
    let user = user_of(db, &f.user_id).await;

    let sess = switch_active_workspace(
        db,
        &kv().await,
        "test-secret",
        &user,
        &f.joined_workspace_id,
        &device(),
    )
    .await
    .expect("switch into just-joined workspace should succeed");

    assert_eq!(sess.workspace_id, Some(f.joined_workspace_id.clone()));
    let updated = user_of(db, &f.user_id).await;
    assert_eq!(updated.last_workspace_id, Some(f.joined_workspace_id));
    assert_ne!(updated.last_workspace_id, Some(f.home_workspace_id));
}

/// `accept_invitation_and_switch` is the single call the `accept_invitation`
/// server fn makes: accept, then switch into the joined workspace.
async fn accept_invitation_and_switch_drops_user_into_joined_workspace(db: &DbPool) {
    let f = fixture(db).await;
    let kv = kv().await;
    let ctx = AcceptInvitationCtx {
        db,
        kv: &kv,
        jwt_secret: "test-secret",
    };

    let sess = accept_invitation_and_switch(
        &ctx,
        &f.invitation_id,
        &f.user_id,
        &f.joined_workspace_id,
        &device(),
    )
    .await
    .expect("accept-and-switch should succeed")
    .expect("switch should succeed and return a session");

    assert_eq!(sess.workspace_id, Some(f.joined_workspace_id.clone()));
    let updated = user_of(db, &f.user_id).await;
    assert_eq!(updated.last_workspace_id, Some(f.joined_workspace_id));
}

/// A second accept of the same invitation is a conflict and creates no second
/// membership or session switch.
async fn accept_invitation_and_switch_conflicts_on_a_second_accept(db: &DbPool) {
    let f = fixture(db).await;
    let kv = kv().await;
    let ctx = AcceptInvitationCtx {
        db,
        kv: &kv,
        jwt_secret: "test-secret",
    };
    accept_invitation_and_switch(&ctx, &f.invitation_id, &f.user_id, &f.joined_workspace_id, &device())
        .await
        .expect("first accept");

    let second = accept_invitation_and_switch(
        &ctx,
        &f.invitation_id,
        &f.user_id,
        &f.joined_workspace_id,
        &device(),
    )
    .await;

    match second {
        Err(krausen_core::Error::Conflict(_)) => {}
        Err(other) => panic!("second accept must be a conflict, got {other:?}"),
        Ok(_) => panic!("second accept must be a conflict, but it succeeded"),
    }
}

/// The enriched lookup returns workspace and inviter names, with no status
/// filter, and `None` for an unknown id.
async fn get_invitation_enriched_returns_names_and_none_for_unknown(db: &DbPool) {
    let f = fixture(db).await;

    let inv = get_invitation_enriched(db, &f.invitation_id)
        .await
        .expect("lookup")
        .expect("invitation exists");
    assert_eq!(inv.invitation_id, f.invitation_id);
    assert_eq!(inv.email, f.user_email);
    assert_eq!(inv.status, "pending");
    assert_eq!(inv.inviter_name.as_deref(), Some("Owner"));
    assert!(inv.workspace_name.is_some(), "workspace name is joined in");

    accept_invitation_for_user(db, &f.invitation_id, &f.user_id)
        .await
        .expect("accept");
    let after = get_invitation_enriched(db, &f.invitation_id)
        .await
        .expect("lookup")
        .expect("still returned after acceptance");
    assert_eq!(after.status, "accepted");

    assert!(get_invitation_enriched(db, "inv-missing")
        .await
        .expect("lookup")
        .is_none());
}

both_backends!(
    switch_rejects_workspace_with_no_active_membership,
    accept_then_switch_drops_user_into_joined_workspace,
    accept_invitation_and_switch_drops_user_into_joined_workspace,
    accept_invitation_and_switch_conflicts_on_a_second_accept,
    get_invitation_enriched_returns_names_and_none_for_unknown,
);
