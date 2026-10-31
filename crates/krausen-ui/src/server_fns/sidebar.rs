// SPDX-License-Identifier: AGPL-3.0-or-later

//! Server functions for the sidebar — user info and workspace switching.

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ssr")]
use super::IntoServerFnError;

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

    // Read theme preference from user's extra_metadata (same source as profile.rs)
    let user = krausen_auth::user_service::get_user_by_id(&ctx.db, &auth.user_id)
        .await
        .into_sfn()?
        .ok_or_else(|| ServerFnError::new("User not found"))?;

    let theme_preference = user
        .extra_metadata
        .as_ref()
        .and_then(|v| v.get("theme"))
        .and_then(|v| v.as_str())
        .unwrap_or("system")
        .to_string();

    Ok(SidebarUser {
        user_id: auth.user_id.clone(),
        workspace_id: auth.workspace.workspace_id.clone(),
        name: auth.name.clone(),
        email: auth.email.clone(),
        workspace_name: auth.workspace.workspace_name.clone(),
        is_personal_mode: ctx.config.is_personal(),
        is_self_hosted: ctx.config.self_hosted,
        theme_preference,
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

    let workspaces = krausen_auth::workspace_service::get_user_workspaces(&ctx.db, &auth.user_id)
        .await
        .into_sfn()?;

    let current_ws_id = auth.workspace.workspace_id.as_deref().unwrap_or("");

    Ok(workspaces
        .into_iter()
        .map(|(ws, _membership)| UserWorkspace {
            is_current: ws.workspace_id == current_ws_id,
            name: ws.name.unwrap_or_else(|| "Unnamed Workspace".to_string()),
            workspace_id: ws.workspace_id,
        })
        .collect())
}

/// Switch the user's active workspace.
#[server(prefix = "/leptos-api")]
pub async fn switch_workspace(workspace_id: String) -> Result<(), ServerFnError> {
    let ctx = super::extract_context()?;
    let auth = super::extract_auth().await?;

    krausen_auth::user_service::update_last_workspace(&ctx.db, &auth.user_id, &workspace_id)
        .await
        .into_sfn()?;

    Ok(())
}
