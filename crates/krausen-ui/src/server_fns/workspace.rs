// SPDX-License-Identifier: AGPL-3.0-or-later

//! Server functions for the Workspace settings page.
//!
//! to `/api/v1/workspaces/*` endpoints. Each function calls the same
//! service-layer code as the existing REST route handlers in
//! `apps/server/src/routes/workspaces.rs`.

use leptos::prelude::*;

use crate::types::WorkspaceSettingsData;
use krausen_types::workspace::WorkspaceRenameOutcome;

// ─────────────────────────────────────────────────────────────────────────────
// Helpers (server-only)
// ─────────────────────────────────────────────────────────────────────────────

// ─────────────────────────────────────────────────────────────────────────────
// Read operations
// ─────────────────────────────────────────────────────────────────────────────

/// Load workspace settings for the admin settings page.
///
/// Returns workspace name. Requires workspace admin role.
#[server(prefix = "/leptos-api")]
pub async fn get_workspace_settings() -> Result<WorkspaceSettingsData, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    let workspace = krausen_app::workspace_snapshot(ac.db(), &actor)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "workspace snapshot failed");
            ServerFnError::new("Unable to load workspace settings")
        })?;
    Ok(WorkspaceSettingsData {
        workspace_name: workspace.name.unwrap_or_default(),
        revision: workspace.revision,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Write operations
// ─────────────────────────────────────────────────────────────────────────────

/// Update the workspace name. Requires admin role.
#[server(prefix = "/leptos-api")]
pub async fn update_workspace_name(
    name: String,
    expected_revision: i64,
    request_id: String,
) -> Result<WorkspaceRenameOutcome, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    Ok(execute_rename(ac.db(), &actor, &name, expected_revision, &request_id).await)
}

#[cfg(feature = "ssr")]
async fn execute_rename(
    db: &krausen_core::DbPool,
    actor: &krausen_app::ActorContext,
    name: &str,
    expected_revision: i64,
    request_id: &str,
) -> WorkspaceRenameOutcome {
    let request_id = match uuid::Uuid::parse_str(request_id) {
        Ok(id) => id,
        Err(_) => return WorkspaceRenameOutcome::Invalid,
    };
    krausen_app::project_rename_result(
        krausen_app::rename_workspace(db, actor, name, expected_revision, request_id).await,
    )
}

// Helpers — delegate to shared extractors in parent module
#[cfg(feature = "ssr")]
use super::AuthenticatedContext;

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    #[tokio::test]
    async fn retry_preserves_browser_request_identity() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../apps/server/migrations-sqlite")
            .run(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO users (user_id, email) VALUES ('u', 'u@example.test')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO workspaces (workspace_id, owner_user_id) VALUES ('w', 'u')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO workspace_users (workspace_id, user_id, role) VALUES ('w', 'u', 'workspace_admin')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ('c','u','browser','test','test',4102444800)").execute(&pool).await.unwrap();
        let db = krausen_core::DbPool::Sqlite(pool);
        let actor = krausen_app::ActorContext {
            actor_id: "u".into(),
            workspace_id: "w".into(),
            credential_id: "c".into(),
            access_expires_at: 4102444800,
        };
        let request = uuid::Uuid::new_v4().to_string();
        assert!(matches!(
            execute_rename(&db, &actor, "Name", 0, &request).await,
            WorkspaceRenameOutcome::Saved { revision: 1 }
        ));
        assert!(matches!(
            execute_rename(&db, &actor, "Name", 0, &request).await,
            WorkspaceRenameOutcome::Saved { revision: 1 }
        ));
        assert!(matches!(
            execute_rename(&db, &actor, "Other", 0, &request).await,
            WorkspaceRenameOutcome::RequestIdentityConflict
        ));
    }
}
