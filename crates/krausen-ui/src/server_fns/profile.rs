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

/// Verify an invitation can be accepted/declined by the authenticated user.
///
/// Pure and DB-free so it's directly unit-testable. Checked in this order:
/// recipient match, then status, then expiry, so the first error surfaced
/// always matches the first thing actually wrong with the invitation.
#[cfg(feature = "ssr")]
fn check_invitation_acceptable(
    inv: &krausen_core::models::WorkspaceInvitation,
    auth_email: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<(), String> {
    if !inv.email.eq_ignore_ascii_case(auth_email) {
        return Err("This invitation is addressed to a different account".into());
    }
    if inv.status != krausen_core::enums::InvitationStatus::Pending {
        return Err("This invitation is no longer pending".into());
    }
    if inv.expires_at < now {
        return Err("This invitation has expired".into());
    }
    Ok(())
}

/// Accept a workspace invitation.
///
/// Validates the invitation is addressed to the authenticated user's email,
/// still pending, and not expired before accepting; without that check any
/// authenticated user could accept any invitation by ID.
#[server(prefix = "/leptos-api")]
pub async fn accept_invitation(invitation_id: String) -> Result<(), ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;

    let inv = krausen_auth::workspace_service::get_invitation(&ctx.db, &invitation_id)
        .await
        .into_sfn()?
        .ok_or_else(|| ServerFnError::new("Invitation not found"))?;
    check_invitation_acceptable(&inv, &auth.email, chrono::Utc::now())
        .map_err(ServerFnError::new)?;

    krausen_auth::workspace_service::accept_invitation_for_user(
        &ctx.db,
        &invitation_id,
        &auth.user_id,
    )
    .await
    .into_sfn()?;

    Ok(())
}

/// Decline a workspace invitation.
///
/// Same recipient/state validation as `accept_invitation`: without it any
/// authenticated user could decline any invitation by ID.
#[server(prefix = "/leptos-api")]
pub async fn decline_invitation(invitation_id: String) -> Result<(), ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;

    let inv = krausen_auth::workspace_service::get_invitation(&ctx.db, &invitation_id)
        .await
        .into_sfn()?
        .ok_or_else(|| ServerFnError::new("Invitation not found"))?;
    check_invitation_acceptable(&inv, &auth.email, chrono::Utc::now())
        .map_err(ServerFnError::new)?;

    krausen_auth::workspace_service::update_invitation_status(
        &ctx.db,
        &invitation_id,
        "declined",
    )
    .await
    .into_sfn()?;

    Ok(())
}

// Helpers — delegate to shared extractors in parent module
#[cfg(feature = "ssr")]
use super::{extract_auth, extract_context, IntoServerFnError};

#[cfg(all(test, feature = "ssr"))]
mod tests {
    //! Rejection-path coverage for `check_invitation_acceptable`: the guard
    //! that stops any authenticated user accepting or declining any invitation
    //! by ID.
    use super::check_invitation_acceptable;
    use krausen_core::enums::InvitationStatus;
    use krausen_core::models::WorkspaceInvitation;

    fn base_invitation(
        status: InvitationStatus,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> WorkspaceInvitation {
        WorkspaceInvitation {
            invitation_id: "inv-test123".to_string(),
            workspace_id: "ws-test123".to_string(),
            email: "recipient@example.com".to_string(),
            role: "workspace_user".to_string(),
            invited_by_user_id: "user-inviter".to_string(),
            status,
            created_at: chrono::Utc::now(),
            expires_at,
            accepted_at: None,
            accepted_by_user_id: None,
        }
    }

    #[test]
    fn accepts_valid_pending_invite_for_recipient() {
        let now = chrono::Utc::now();
        let inv = base_invitation(InvitationStatus::Pending, now + chrono::Duration::days(1));
        assert_eq!(
            check_invitation_acceptable(&inv, "recipient@example.com", now),
            Ok(())
        );
    }

    #[test]
    fn rejects_wrong_recipient() {
        let now = chrono::Utc::now();
        let inv = base_invitation(InvitationStatus::Pending, now + chrono::Duration::days(1));
        assert_eq!(
            check_invitation_acceptable(&inv, "attacker@example.com", now),
            Err("This invitation is addressed to a different account".to_string())
        );
    }

    #[test]
    fn rejects_expired() {
        let now = chrono::Utc::now();
        let inv = base_invitation(InvitationStatus::Pending, now - chrono::Duration::days(1));
        assert_eq!(
            check_invitation_acceptable(&inv, "recipient@example.com", now),
            Err("This invitation has expired".to_string())
        );
    }

    #[test]
    fn rejects_already_accepted() {
        let now = chrono::Utc::now();
        let inv = base_invitation(InvitationStatus::Accepted, now + chrono::Duration::days(1));
        assert_eq!(
            check_invitation_acceptable(&inv, "recipient@example.com", now),
            Err("This invitation is no longer pending".to_string())
        );
    }

    #[test]
    fn rejects_cancelled() {
        let now = chrono::Utc::now();
        let inv = base_invitation(InvitationStatus::Cancelled, now + chrono::Duration::days(1));
        assert_eq!(
            check_invitation_acceptable(&inv, "recipient@example.com", now),
            Err("This invitation is no longer pending".to_string())
        );
    }
}
