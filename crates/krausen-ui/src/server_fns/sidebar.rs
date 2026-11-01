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

    let actor = super::verified_account_actor(&auth)?;
    let profile = match krausen_app::account_read::project_profile(krausen_app::account_read::profile(&ctx.db, &actor).await) {
        krausen_types::account_read::AccountProfileOutcome::Available { profile } => profile,
        _ => return Err(ServerFnError::new("Unable to load sidebar user")),
    };

    Ok(SidebarUser {
        user_id: auth.user_id.clone(),
        workspace_id: auth.workspace.workspace_id.clone(),
        name: auth.name.clone(),
        email: auth.email.clone(),
        workspace_name: auth.workspace.workspace_name.clone(),
        is_personal_mode: ctx.config.is_personal(),
        is_self_hosted: ctx.config.self_hosted,
        theme_preference: profile.theme,
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
    list_shared_workspaces(&ctx.db, &auth).await
}

#[cfg(feature = "ssr")]
async fn list_shared_workspaces(
    db: &krausen_core::DbPool,
    auth: &krausen_auth::middleware::AuthUser,
) -> Result<Vec<UserWorkspace>, ServerFnError> {
    let actor = super::verified_account_actor(&auth)?;
    let current_ws_id = auth.workspace.workspace_id.as_deref().unwrap_or("");
    let mut cursor = None;
    let mut results = Vec::new();
    loop {
        let outcome = krausen_app::account_read::project_workspaces(
            krausen_app::account_read::workspaces(db, &actor, cursor.as_deref(), 100).await,
        );
        let krausen_types::account_read::AccountWorkspaceListOutcome::Available { page } = outcome else {
            return Err(ServerFnError::new("Unable to load workspaces"));
        };
        results.extend(page.items.into_iter().map(|ws| UserWorkspace {
            is_current: ws.workspace_id == current_ws_id,
            name: ws.name.unwrap_or_else(|| "Unnamed Workspace".to_string()),
            workspace_id: ws.workspace_id,
        }));
        cursor = page.next_cursor;
        if cursor.is_none() { break; }
    }
    Ok(results)
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    #[tokio::test]
    async fn workspace_sidebar_follows_all_pages() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("../../apps/server/migrations-sqlite").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO users (user_id,email) VALUES ('u','u@example.test')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ('c','u','browser','test','test',4102444800)").execute(&pool).await.unwrap();
        for i in 0..101 {
            let id = format!("w-{i:03}");
            sqlx::query("INSERT INTO workspaces (workspace_id,owner_user_id) VALUES ($1,'u')").bind(&id).execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO workspace_users (workspace_id,user_id,role) VALUES ($1,'u','workspace_admin')").bind(&id).execute(&pool).await.unwrap();
        }
        let db = krausen_core::DbPool::Sqlite(pool);
        let auth = krausen_auth::middleware::AuthUser {
            user_id: "u".into(), credential_id: "c".into(), email: "u@example.test".into(),
            name: None, roles: vec![], active: true, verified: true,
            workspace: krausen_auth::middleware::WorkspaceContext { workspace_id: Some("w-100".into()), ..Default::default() },
            token_exp: Some(4102444800), token_jti: None,
        };
        let list = list_shared_workspaces(&db, &auth).await.unwrap();
        assert_eq!(list.len(), 101);
        assert_eq!(list.iter().filter(|item| item.is_current).count(), 1);
        assert!(list.iter().any(|item| item.workspace_id == "w-100" && item.is_current));
    }
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
