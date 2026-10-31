// SPDX-License-Identifier: AGPL-3.0-or-later

//! Server function for user context.

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ssr")]
use super::{extract_auth, extract_context};

/// Combined user, workspace, and capability context.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserContext {
    pub user_id: String,
    pub email: String,
    pub name: Option<String>,
    pub workspace_id: Option<String>,
    pub workspace_name: Option<String>,
    pub workspace_roles: Vec<String>,
    pub is_owner: bool,
    pub is_personal_mode: bool,
    pub is_self_hosted: bool,
}

/// Load the authenticated user's full context.
#[server(prefix = "/leptos-api")]
pub async fn get_user_context() -> Result<UserContext, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;
    let actor = super::verified_account_actor(&auth)?;
    let current = krausen_app::account::context(&ctx.db, &actor)
        .await
        .map_err(|_| ServerFnError::new("Unable to load account context"))?;

    Ok(UserContext {
        user_id: current.profile.user_id,
        email: current.profile.email,
        name: current.profile.name,
        workspace_id: current
            .workspace
            .as_ref()
            .map(|workspace| workspace.workspace_id.clone()),
        workspace_name: current
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.name.clone()),
        workspace_roles: current
            .workspace
            .as_ref()
            .map(|workspace| vec![workspace.role.clone()])
            .unwrap_or_default(),
        is_owner: current.is_owner,
        is_personal_mode: ctx.config.is_personal(),
        is_self_hosted: ctx.config.self_hosted,
    })
}
