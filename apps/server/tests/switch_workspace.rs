// SPDX-License-Identifier: AGPL-3.0-or-later
//! The `switch_workspace` server function over real HTTP, on SQLite and
//! PostgreSQL: switching needs an active membership in the target workspace, and
//! a successful switch re-mints the browser session for it (new cookies carrying
//! the new workspace).

mod common;

use common::{Account, TestServer, account};
use krausen_auth::{jwt, user_service};
use leptos::server_fn::ServerFn;
use reqwest::StatusCode;

fn switch_path() -> &'static str {
    <krausen_ui::server_fns::sidebar::SwitchWorkspace as ServerFn>::PATH
}

async fn switch(srv: &TestServer, who: &Account, workspace_id: &str) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!("{}{}", srv.base, switch_path()))
        .header("content-type", "application/x-www-form-urlencoded")
        .header("cookie", format!("access_token={}", who.access_token))
        .body(format!("workspace_id={workspace_id}"))
        .send()
        .await
        .unwrap()
}

fn set_cookie(response: &reqwest::Response, name: &str) -> Option<String> {
    response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(|v| v.strip_prefix(&format!("{name}=")))
        .and_then(|v| v.split(';').next())
        .map(str::to_owned)
}

async fn last_workspace(srv: &TestServer, user_id: &str) -> Option<String> {
    user_service::get_user_by_id(&srv.db, user_id)
        .await
        .unwrap()
        .unwrap()
        .last_workspace_id
}

async fn a_member_can_switch_and_the_session_is_re_minted(srv: &TestServer) {
    let who = account(srv).await;
    let second = user_service::create_workspace_for_user(
        &srv.db,
        &who.user_id,
        Some("Second"),
        &who.email,
        None,
    )
    .await
    .unwrap();
    let response = switch(srv, &who, &second).await;
    assert_eq!(response.status(), StatusCode::OK);
    let access = set_cookie(&response, "access_token").expect("re-minted access cookie");
    assert!(set_cookie(&response, "refresh_token").is_some(), "re-minted refresh cookie");
    let claims = jwt::validate_token(&access, &srv.secret).unwrap().claims;
    assert_eq!(claims.extra["workspace_id"], second.as_str());
    assert_eq!(last_workspace(srv, &who.user_id).await.as_deref(), Some(second.as_str()));
}

/// A refused switch mints no session and leaves the saved workspace unchanged.
async fn assert_refused(srv: &TestServer, who: &Account, target: &str, why: &str) {
    let before = last_workspace(srv, &who.user_id).await;
    let response = switch(srv, who, target).await;
    assert!(set_cookie(&response, "access_token").is_none(), "{why}: no session minted");
    assert!(set_cookie(&response, "refresh_token").is_none(), "{why}: no session minted");
    assert_eq!(
        last_workspace(srv, &who.user_id).await,
        before,
        "{why}: the saved workspace is unchanged"
    );
    assert_ne!(
        last_workspace(srv, &who.user_id).await.as_deref(),
        Some(target),
        "{why}: the target was not saved"
    );
}

async fn a_non_member_cannot_switch_into_another_workspace(srv: &TestServer) {
    let who = account(srv).await;
    let other = account(srv).await;
    assert_refused(srv, &who, &other.workspace_id, "non-member").await;
}

async fn a_removed_member_cannot_switch_back(srv: &TestServer) {
    let who = account(srv).await;
    let second = user_service::create_workspace_for_user(
        &srv.db,
        &who.user_id,
        Some("Second"),
        &who.email,
        None,
    )
    .await
    .unwrap();
    // The account's saved workspace is its first one; membership of the second
    // is removed, so switching to it must be refused.
    user_service::update_last_workspace(&srv.db, &who.user_id, &who.workspace_id)
        .await
        .unwrap();
    krausen_core::db_execute!(
        &srv.db,
        "UPDATE workspace_users SET active = $1 WHERE workspace_id = $2 AND user_id = $3",
        &false,
        &second,
        &who.user_id
    )
    .unwrap();
    assert_refused(srv, &who, &second, "removed member").await;
}

async fn a_pending_invitation_is_not_membership(srv: &TestServer) {
    let who = account(srv).await;
    let owner = account(srv).await;
    krausen_auth::workspace_service::create_invitation(
        &srv.db,
        "invitation-pending",
        &owner.workspace_id,
        &who.email,
        "workspace_user",
        &owner.user_id,
        chrono::Utc::now() + chrono::Duration::days(1),
    )
    .await
    .unwrap();
    assert_refused(srv, &who, &owner.workspace_id, "pending invitation").await;
}

common::both_backends!(
    a_member_can_switch_and_the_session_is_re_minted,
    a_non_member_cannot_switch_into_another_workspace,
    a_removed_member_cannot_switch_back,
    a_pending_invitation_is_not_membership,
);
