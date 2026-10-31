// SPDX-License-Identifier: AGPL-3.0-or-later

//! Server functions for the sidebar — user info and workspace switching.

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// User info for the sidebar user menu.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SidebarUser {
    pub user_id: String,
    pub workspace_id: Option<String>,
    pub name: Option<String>,
    pub email: String,
    pub workspace_name: Option<String>,
    pub is_personal_mode: bool,
    /// Whether the server is running in self-hosted mode.
    pub is_self_hosted: bool,
    /// User's theme preference: "light", "dark", or "system".
    pub theme_preference: String,
}

/// Load current user info for the sidebar user menu.
#[server(prefix = "/leptos-api")]
pub async fn get_sidebar_user() -> Result<SidebarUser, ServerFnError> {
    let auth = super::extract_auth().await?;
    let ctx = super::extract_context()?;

    let actor = super::verified_account_actor(&auth)?;
    let current = krausen_app::account::context(&ctx.db, &actor)
        .await
        .map_err(|_| ServerFnError::new("Unable to load account"))?;

    Ok(SidebarUser {
        user_id: current.profile.user_id,
        workspace_id: current
            .workspace
            .as_ref()
            .map(|workspace| workspace.workspace_id.clone()),
        name: current.profile.name,
        email: current.profile.email,
        workspace_name: current
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.name.clone()),
        is_personal_mode: ctx.config.is_personal(),
        is_self_hosted: ctx.config.self_hosted,
        theme_preference: current.profile.theme,
    })
}

/// A workspace the user belongs to.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserWorkspace {
    pub workspace_id: String,
    pub name: String,
    pub is_current: bool,
}

/// List all workspaces the current user belongs to.
#[server(prefix = "/leptos-api")]
pub async fn list_user_workspaces() -> Result<Vec<UserWorkspace>, ServerFnError> {
    let ctx = super::extract_context()?;
    let auth = super::extract_auth().await?;

    let actor = super::verified_account_actor(&auth)?;
    let workspaces = krausen_app::account::workspaces(&ctx.db, &actor)
        .await
        .map_err(|_| ServerFnError::new("Unable to load workspaces"))?;

    let current_ws_id = auth.workspace.workspace_id.as_deref().unwrap_or("");

    Ok(workspaces
        .into_iter()
        .map(|ws| UserWorkspace {
            is_current: ws.workspace_id == current_ws_id,
            name: ws.name.unwrap_or_else(|| "Unnamed Workspace".to_string()),
            workspace_id: ws.workspace_id,
        })
        .collect())
}

/// Switch the user's active workspace.
#[server(prefix = "/leptos-api")]
pub async fn switch_workspace(
    workspace_id: String,
    request_id: String,
) -> Result<(), ServerFnError> {
    let ctx = super::extract_context()?;
    let auth = super::extract_auth().await?;

    let actor = super::verified_account_actor(&auth)?;
    let request_id =
        uuid::Uuid::parse_str(&request_id).map_err(|_| ServerFnError::new("Invalid request ID"))?;
    let selected =
        krausen_app::account::select_workspace(&ctx.db, &actor, &workspace_id, request_id)
            .await
            .map_err(|_| ServerFnError::new("Unable to switch workspace"))?;
    let mut claims = std::collections::HashMap::new();
    claims.insert("user_id".into(), serde_json::json!(auth.user_id));
    claims.insert("email".into(), serde_json::json!(auth.email));
    claims.insert("name".into(), serde_json::json!(auth.name));
    claims.insert("roles".into(), serde_json::json!(auth.roles));
    claims.insert(
        "credential_id".into(),
        serde_json::json!(auth.credential_id),
    );
    claims.insert(
        "workspace_id".into(),
        serde_json::json!(selected.workspace_id),
    );
    claims.insert("workspace_name".into(), serde_json::json!(selected.name));
    claims.insert("workspace_roles".into(), serde_json::json!([selected.role]));
    claims.insert(
        "iss".into(),
        serde_json::json!(krausen_auth::jwt::BROWSER_ISSUER),
    );
    claims.insert(
        "aud".into(),
        serde_json::json!(krausen_auth::jwt::BROWSER_AUDIENCE),
    );
    let token = krausen_auth::jwt::create_access_token_until(
        &actor.actor_id,
        &ctx.auth_state.jwt_secret,
        actor.access_expires_at,
        claims,
    )
    .map_err(|_| ServerFnError::new("Unable to switch workspace"))?;
    let mut headers = axum::http::HeaderMap::new();
    krausen_auth::cookies::set_token_cookies(&mut headers, Some(&token), None);
    let response = expect_context::<leptos_axum::ResponseOptions>();
    for value in headers.get_all(axum::http::header::SET_COOKIE) {
        response.append_header(axum::http::header::SET_COOKIE, value.clone());
    }

    Ok(())
}
