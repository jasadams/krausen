// SPDX-License-Identifier: AGPL-3.0-or-later

//! Server functions for the Profile settings page.
//!
//! Each function calls the same service-layer code as the existing REST routes.

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

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

    // Accept, then drop the user into the workspace they just joined.
    // Switching re-mints the session JWT for the new workspace; the client
    // hard-navigates afterward, so the fresh cookie takes effect on the next
    // request. Acceptance errors propagate; the switch is best-effort: a
    // failure there must not surface as an accept failure, since the
    // membership is already committed and the user can switch manually. See
    // `accept_invitation_and_switch` for the full contract.
    let headers: axum::http::HeaderMap = leptos_axum::extract()
        .await
        .map_err(|e| ServerFnError::new(format!("Failed to extract headers: {e}")))?;
    let device = super::auth::extract_device_info(&headers);

    let session = match ctx.kv.clone() {
        Some(kv) => {
            let accept_ctx = krausen_auth::workspace_service::AcceptInvitationCtx {
                db: &ctx.db,
                kv: &kv,
                jwt_secret: &ctx.config.jwt_secret,
            };
            krausen_auth::workspace_service::accept_invitation_and_switch(
                &accept_ctx,
                &invitation_id,
                &auth.user_id,
                &inv.workspace_id,
                &device,
            )
            .await
            .into_sfn()?
        }
        None => {
            // No KV (single-instance/test) — still accept the invite; just can't
            // re-mint a session for the new workspace.
            tracing::warn!(
                user_id = %auth.user_id,
                "invite accepted but KV store unavailable; skipping active-workspace switch"
            );
            krausen_auth::workspace_service::accept_invitation_for_user(
                &ctx.db,
                &invitation_id,
                &auth.user_id,
            )
            .await
            .into_sfn()?;
            None
        }
    };

    if let Some(session) = session {
        super::auth::set_session_cookies(&session);
    }

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

/// Invitation details for display on the accept-invite page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InvitationDisplay {
    pub invitation_id: String,
    pub workspace_name: String,
    pub inviter_name: String,
    pub role: String,
    pub expires_at: String,
    pub status: String,
}

/// Fetch invitation details for the accept-invite page.
///
/// Returns `Ok(None)` both when the invitation doesn't exist AND when it
/// exists but isn't addressed to the authenticated user — the latter case
/// must not leak invitation existence to a non-recipient.
#[server(prefix = "/leptos-api")]
pub async fn get_invitation_for_accept(
    invitation_id: String,
) -> Result<Option<InvitationDisplay>, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;

    let inv = krausen_auth::workspace_service::get_invitation_enriched(&ctx.db, &invitation_id)
        .await
        .into_sfn()?;
    let Some(inv) = inv else {
        return Ok(None);
    };
    if !inv.email.eq_ignore_ascii_case(&auth.email) {
        return Ok(None);
    }

    Ok(Some(InvitationDisplay {
        invitation_id: inv.invitation_id,
        workspace_name: inv.workspace_name.unwrap_or_default(),
        inviter_name: inv.inviter_name.unwrap_or_default(),
        role: krausen_core::constants::humanize_workspace_role(&inv.role).to_string(),
        expires_at: inv.expires_at.to_rfc3339(),
        status: inv.status,
    }))
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
