// SPDX-License-Identifier: AGPL-3.0-or-later

//! Server functions for the Profile settings page.
//!
//! Each function calls the same service-layer code as the existing REST routes.

use leptos::prelude::*;

use crate::types::{InvitationData, ProfileData};

// ─────────────────────────────────────────────────────────────────────────────
// Read operations (called on page load via Resource)
// ─────────────────────────────────────────────────────────────────────────────

/// Load the current user's profile data.
///
/// Combines user info and preferences into a single response —
/// replacing multiple separate REST calls.
#[server(prefix = "/leptos-api")]
pub async fn get_profile() -> Result<ProfileData, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;

    let user = krausen_auth::user_service::get_user_by_id(&ctx.db, &auth.user_id)
        .await
        .into_sfn()?
        .ok_or_else(|| ServerFnError::new("User not found"))?;

    let metadata = user.extra_metadata.as_ref().and_then(|v| v.as_object());

    let theme = metadata
        .and_then(|m| m.get("theme"))
        .and_then(|v| v.as_str())
        .unwrap_or("system")
        .to_string();

    Ok(ProfileData {
        user_id: user.user_id,
        email: user.email,
        name: user.name,
        theme,
        is_personal_mode: ctx.config.is_personal(),
        is_self_hosted: ctx.config.self_hosted,
    })
}

/// Load pending workspace invitations for the current user.
#[server(prefix = "/leptos-api")]
pub async fn get_pending_invitations() -> Result<Vec<InvitationData>, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;

    let invitations =
        krausen_auth::workspace_service::get_pending_invitations_for_email(&ctx.db, &auth.email)
            .await
            .into_sfn()?;

    Ok(invitations
        .into_iter()
        .map(|inv| InvitationData {
            invitation_id: inv.invitation_id,
            workspace_id: inv.workspace_id,
            email: inv.email,
            role: inv.role.to_string(),
            created_at: inv.created_at.to_rfc3339(),
            expires_at: inv.expires_at.to_rfc3339(),
        })
        .collect())
}

// ─────────────────────────────────────────────────────────────────────────────
// Write operations (called on user interaction via Action)
// ─────────────────────────────────────────────────────────────────────────────

/// Update the user's display name.
#[server(prefix = "/leptos-api")]
pub async fn update_profile_name(name: String) -> Result<(), ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;

    let trimmed = name.trim().to_string();
    if trimmed.is_empty() {
        return Err(ServerFnError::new("Name cannot be empty"));
    }

    krausen_auth::user_service::update_user_name(&ctx.db, &auth.user_id, &trimmed)
        .await
        .into_sfn()?;

    Ok(())
}

/// Update user theme preference (light, dark, or system).
#[server(prefix = "/leptos-api")]
pub async fn update_theme(theme: String) -> Result<(), ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;

    if !["light", "dark", "system"].contains(&theme.as_str()) {
        return Err(ServerFnError::new("Theme must be light, dark, or system"));
    }

    let metadata = serde_json::json!({ "theme": theme });
    krausen_auth::user_service::update_extra_metadata(&ctx.db, &auth.user_id, &metadata)
        .await
        .into_sfn()?;

    Ok(())
}

/// Accept a workspace invitation.
#[server(prefix = "/leptos-api")]
pub async fn accept_invitation(invitation_id: String, request_id: String) -> Result<krausen_types::invitation::InvitationResponseOutcome, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;
    respond_to_invitation(&ctx.db, &auth, &invitation_id, &request_id, krausen_types::invitation::InvitationAction::Accept).await
}

/// Decline a workspace invitation.
#[server(prefix = "/leptos-api")]
pub async fn decline_invitation(invitation_id: String, request_id: String) -> Result<krausen_types::invitation::InvitationResponseOutcome, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;
    respond_to_invitation(&ctx.db, &auth, &invitation_id, &request_id, krausen_types::invitation::InvitationAction::Decline).await
}

#[cfg(feature = "ssr")]
async fn respond_to_invitation(
    db: &krausen_core::DbPool,
    auth: &krausen_auth::middleware::AuthUser,
    invitation_id: &str,
    request_id: &str,
    action: krausen_types::invitation::InvitationAction,
) -> Result<krausen_types::invitation::InvitationResponseOutcome, ServerFnError> {
    let Some(expiry) = auth.token_exp else { return Err(ServerFnError::new("Authentication required")) };
    let Ok(request_id) = uuid::Uuid::parse_str(request_id) else { return Ok(krausen_types::invitation::InvitationResponseOutcome::Invalid) };
    let actor = krausen_app::invitation::AccountActor {
        actor_id: auth.user_id.clone(),
        credential_id: auth.credential_id.clone(),
        access_expires_at: expiry,
    };
    Ok(krausen_app::invitation::project(krausen_app::invitation::respond(db, &actor, invitation_id, action, request_id).await))
}

// Helpers — delegate to shared extractors in parent module
#[cfg(feature = "ssr")]
use super::{extract_auth, extract_context, IntoServerFnError};

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use krausen_types::invitation::{InvitationAction, InvitationResponseOutcome};

    #[tokio::test]
    async fn invitation_adapter_uses_server_owned_recipient() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("../../apps/server/migrations-sqlite").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO users (user_id,email) VALUES ('owner','owner@example.test'),('recipient','recipient@example.test'),('other','other@example.test')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO workspaces (workspace_id,owner_user_id) VALUES ('w','owner')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ('c','other','browser','test','test',4102444800)").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO workspace_invitations (invitation_id,workspace_id,email,role,invited_by_user_id,expires_at) VALUES ('i','w','recipient@example.test','workspace_user','owner','2099-01-01T00:00:00Z')").execute(&pool).await.unwrap();
        let db = krausen_core::DbPool::Sqlite(pool);
        let auth = krausen_auth::middleware::AuthUser {
            user_id: "other".into(), credential_id: "c".into(), email: "recipient@example.test".into(),
            name: None, roles: vec![], active: true, verified: true,
            workspace: Default::default(), token_exp: Some(4102444800), token_jti: None,
        };
        let result = respond_to_invitation(&db, &auth, "i", &uuid::Uuid::new_v4().to_string(), InvitationAction::Decline).await.unwrap();
        assert_eq!(result, InvitationResponseOutcome::NotFound);
    }
}
