// SPDX-License-Identifier: AGPL-3.0-or-later

//! Server functions for team management.
//!
//! to `/api/v1/workspaces/members`, `/api/v1/workspaces/invitations`,
//! and `/api/v1/workspaces/ownership/transfers` endpoints.
//!
//! Each function calls the same service-layer code as the existing REST
//! route handlers in `apps/server/src/routes/workspaces.rs`.

use leptos::prelude::*;

#[cfg(feature = "ssr")]
use crate::types::{OwnershipTransferData, TeamInvitation};
use crate::types::{OwnershipTransferList, TeamInvitationList, TeamMember};

// ─────────────────────────────────────────────────────────────────────────────
// Helpers (server-only)
// ─────────────────────────────────────────────────────────────────────────────

// ─────────────────────────────────────────────────────────────────────────────
// Members
// ─────────────────────────────────────────────────────────────────────────────

/// List all workspace members with user details.
///
/// Mirrors `GET /api/v1/workspaces/members` in workspaces.rs.
#[server(prefix = "/leptos-api")]
pub async fn list_workspace_members() -> Result<Vec<TeamMember>, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;

    let actor = super::verified_actor(&ac.auth)?;
    let snapshot = krausen_app::team::members(ac.db(), &actor)
        .await
        .map_err(|_| ServerFnError::new("Unable to load team"))?;
    Ok(snapshot
        .members
        .into_iter()
        .map(|m| TeamMember {
            user_id: m.user_id,
            email: m.email,
            name: m.name,
            role: m.role,
            is_owner: m.is_owner,
            joined_at: m.joined_at,
            revision: snapshot.revision,
        })
        .collect())
}

/// Update a member's role. Requires admin.
///
/// Mirrors `PATCH /api/v1/workspaces/members/{id}/role` in workspaces.rs.
#[server(prefix = "/leptos-api")]
pub async fn update_member_role(
    user_id: String,
    role: String,
    expected_revision: i64,
    request_id: String,
) -> Result<krausen_types::account::TeamChangeOutcome, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    let role = match role.as_str() {
        "admin" => "workspace_admin",
        "user" => "workspace_user",
        _ => return Err(ServerFnError::new("Invalid role")),
    };
    let request_id = uuid::Uuid::parse_str(&request_id)
        .map_err(|_| ServerFnError::new("Invalid request identity"))?;
    Ok(krausen_app::team::project_change(
        krausen_app::team::change(
            ac.db(),
            &actor,
            krausen_types::account::TeamChange::SetRole {
                user_id,
                role: role.into(),
            },
            expected_revision,
            request_id,
        )
        .await,
    ))
}

/// Remove a member under current workspace authority and revision.
#[server(prefix = "/leptos-api")]
pub async fn remove_member(
    user_id: String,
    expected_revision: i64,
    request_id: String,
) -> Result<krausen_types::account::TeamChangeOutcome, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    let request_id = uuid::Uuid::parse_str(&request_id)
        .map_err(|_| ServerFnError::new("Invalid request identity"))?;
    Ok(krausen_app::team::project_change(
        krausen_app::team::change(
            ac.db(),
            &actor,
            krausen_types::account::TeamChange::Remove { user_id },
            expected_revision,
            request_id,
        )
        .await,
    ))
}

// ─────────────────────────────────────────────────────────────────────────────
// Invitations
// ─────────────────────────────────────────────────────────────────────────────

/// List pending invitations for the workspace. Requires admin.
///
/// Mirrors `GET /api/v1/workspaces/invitations` in workspaces.rs.
#[server(prefix = "/leptos-api")]
pub async fn list_workspace_invitations() -> Result<TeamInvitationList, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    let page = krausen_app::team::invitations(ac.db(), &actor)
        .await
        .map_err(|_| ServerFnError::new("Unable to load invitations"))?;
    Ok(TeamInvitationList {
        invitations: page
            .items
            .into_iter()
            .map(|inv| TeamInvitation {
                invitation_id: inv.invitation_id,
                email: inv.email,
                role: inv.role,
                status: inv.status,
                created_at: inv.created_at,
                expires_at: inv.expires_at,
            })
            .collect(),
        truncated: page.truncated,
    })
}

/// Create a new invitation. Requires admin.
///
/// Mirrors `POST /api/v1/workspaces/invitations` in workspaces.rs.
#[server(prefix = "/leptos-api")]
pub async fn invite_member(
    email: String,
    role: String,
    expected_revision: i64,
    request_id: String,
) -> Result<krausen_types::account::InvitationCreateOutcome, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    let role = match role.as_str() {
        "admin" => "workspace_admin",
        "user" => "workspace_user",
        _ => return Err(ServerFnError::new("Invalid role")),
    };
    let request_id = uuid::Uuid::parse_str(&request_id)
        .map_err(|_| ServerFnError::new("Invalid request identity"))?;
    let mail_available = krausen_app::invitation::delivery_available(
        krausen_auth::email_service::EmailService::from_env().is_configured(),
        &ac.ctx.config.base_url,
    );
    Ok(krausen_app::team::project_invite(
        krausen_app::team::invite(
            ac.db(),
            &actor,
            &email,
            role,
            expected_revision,
            request_id,
            mail_available,
        )
        .await,
    ))
}

/// Cancel a pending invitation. Requires admin.
///
/// Mirrors `DELETE /api/v1/workspaces/invitations/{id}` in workspaces.rs.
#[server(prefix = "/leptos-api")]
pub async fn cancel_invitation(
    invitation_id: String,
    expected_revision: i64,
    request_id: String,
) -> Result<krausen_types::account::TeamChangeOutcome, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    let request_id = uuid::Uuid::parse_str(&request_id)
        .map_err(|_| ServerFnError::new("Invalid request identity"))?;
    Ok(krausen_app::team::project_change(
        krausen_app::invitation::cancel(
            ac.db(),
            &actor,
            &invitation_id,
            expected_revision,
            request_id,
        )
        .await,
    ))
}

// ─────────────────────────────────────────────────────────────────────────────
// Ownership transfers
// ─────────────────────────────────────────────────────────────────────────────

/// List ownership transfers for the current user.
///
/// Mirrors `GET /api/v1/workspaces/ownership/transfers` in workspaces.rs.
#[server(prefix = "/leptos-api")]
pub async fn list_ownership_transfers() -> Result<OwnershipTransferList, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;

    let actor = super::verified_actor(&ac.auth)?;
    let page = krausen_app::ownership::list(ac.db(), &actor)
        .await
        .map_err(|_| ServerFnError::new("Unable to load transfers"))?;
    Ok(OwnershipTransferList {
        transfers: page
            .items
            .into_iter()
            .map(|t| OwnershipTransferData {
                transfer_id: t.transfer_id,
                from_user_id: t.from_user_id.clone(),
                from_user_email: t.from_user_email,
                to_user_id: t.to_user_id.clone(),
                to_user_email: t.to_user_email,
                status: t.status,
                created_at: t.created_at,
                expires_at: t.expires_at,
                is_initiator: t.from_user_id == actor.actor_id,
                is_recipient: t.to_user_id == actor.actor_id,
            })
            .collect(),
        truncated: page.truncated,
    })
}

/// Cancel an ownership transfer. Only the initiator can cancel.
///
/// Mirrors `DELETE /api/v1/workspaces/ownership/transfer/{id}` in workspaces.rs.
#[server(prefix = "/leptos-api")]
pub async fn cancel_ownership_transfer(
    transfer_id: String,
    expected_revision: i64,
    request_id: String,
) -> Result<krausen_types::account::TeamChangeOutcome, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    let request_id = uuid::Uuid::parse_str(&request_id)
        .map_err(|_| ServerFnError::new("Invalid request identity"))?;
    Ok(krausen_app::team::project_change(
        krausen_app::ownership::cancel(
            ac.db(),
            &actor,
            &transfer_id,
            expected_revision,
            request_id,
        )
        .await,
    ))
}

/// Begin an ownership transfer to a current workspace member.
#[server(prefix = "/leptos-api")]
pub async fn initiate_ownership_transfer(
    to_user_id: String,
    expected_revision: i64,
    request_id: String,
) -> Result<krausen_types::account::OwnershipStartOutcome, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    let request_id = uuid::Uuid::parse_str(&request_id)
        .map_err(|_| ServerFnError::new("Invalid request identity"))?;
    let mail_available = krausen_app::invitation::delivery_available(
        krausen_auth::email_service::EmailService::from_env().is_configured(),
        &ac.ctx.config.base_url,
    );
    Ok(krausen_app::ownership::project_start(
        krausen_app::ownership::start(
            ac.db(),
            &actor,
            &to_user_id,
            expected_revision,
            request_id,
            mail_available,
        )
        .await,
    ))
}

// Helpers — delegate to shared extractors in parent module
#[cfg(feature = "ssr")]
use super::AuthenticatedContext;
