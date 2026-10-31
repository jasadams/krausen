// SPDX-License-Identifier: AGPL-3.0-or-later

//! Server functions for the Profile settings page.
//!
//! Each function calls the same service-layer code as the existing REST routes.

use leptos::prelude::*;

use crate::types::{InvitationData, InvitationDataList, ProfileData};

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

    let actor = super::verified_account_actor(&auth)?;
    let user = krausen_app::account::profile(&ctx.db, &actor)
        .await
        .map_err(|_| ServerFnError::new("Unable to load profile"))?;
    Ok(ProfileData {
        user_id: user.user_id,
        email: user.email,
        name: user.name,
        theme: user.theme,
        revision: user.revision,
        is_personal_mode: ctx.config.is_personal(),
        is_self_hosted: ctx.config.self_hosted,
    })
}

/// Begin mailbox proof without disclosing whether invitations exist.
#[server(prefix = "/leptos-api")]
pub async fn begin_invitation_discovery(
    request_id: String,
) -> Result<krausen_types::account::PendingInteraction, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;
    let actor = super::verified_account_actor(&auth)?;
    let request = uuid::Uuid::parse_str(&request_id)
        .map_err(|_| ServerFnError::new("Invalid request identity"))?;
    let mail_available = krausen_app::invitation::delivery_available(
        krausen_auth::email_service::EmailService::from_env().is_configured(),
        &ctx.config.base_url,
    );
    krausen_app::invitation::begin_discovery(&ctx.db, &actor, request, mail_available)
        .await
        .map_err(invitation_error)
}

/// Read invitations only after the current credential has proved mailbox control.
#[server(prefix = "/leptos-api")]
pub async fn get_pending_invitations(
    discovery_id: String,
) -> Result<InvitationDataList, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;
    let actor = super::verified_account_actor(&auth)?;
    let page = krausen_app::invitation::list(&ctx.db, &actor, &discovery_id)
        .await
        .map_err(invitation_error)?;
    Ok(InvitationDataList {
        invitations: page
            .items
            .into_iter()
            .map(|inv| InvitationData {
                invitation_id: inv.invitation_id,
                workspace_id: inv.workspace_id,
                email: inv.email,
                role: inv.role,
                created_at: inv.created_at,
                expires_at: inv.expires_at,
            })
            .collect(),
        truncated: page.truncated,
    })
}

/// Begin an action-bound mailbox proof; no membership change occurs yet.
#[server(prefix = "/leptos-api")]
pub async fn begin_invitation_response(
    workspace_id: String,
    invitation_id: String,
    discovery_id: String,
    action: krausen_types::account::InvitationAction,
    request_id: String,
) -> Result<krausen_types::account::PendingInteraction, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;
    let mut actor = super::verified_account_actor(&auth)?;
    actor.workspace_id = workspace_id;
    let request = uuid::Uuid::parse_str(&request_id)
        .map_err(|_| ServerFnError::new("Invalid request identity"))?;
    let mail_available = krausen_app::invitation::delivery_available(
        krausen_auth::email_service::EmailService::from_env().is_configured(),
        &ctx.config.base_url,
    );
    krausen_app::invitation::begin(
        &ctx.db,
        &actor,
        &invitation_id,
        &discovery_id,
        action,
        request,
        mail_available,
    )
    .await
    .map_err(invitation_error)
}

#[cfg(feature = "ssr")]
fn invitation_error(error: krausen_app::Error) -> ServerFnError {
    match error {
        krausen_app::Error::DeliveryUnavailable => ServerFnError::new(
            "Email delivery is unavailable. Please try again when it is configured.",
        ),
        krausen_app::Error::Forbidden => {
            ServerFnError::new("Invitation or confirmation unavailable.")
        }
        other => {
            tracing::warn!(%other, "invitation operation failed");
            ServerFnError::new("Invitation operation unavailable.")
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Write operations (called on user interaction via Action)
// ─────────────────────────────────────────────────────────────────────────────

/// Update the user's display name.
#[server(prefix = "/leptos-api")]
pub async fn update_profile(
    change_json: String,
    expected_revision: i64,
    request_id: String,
) -> Result<krausen_types::account::ProfileSaveOutcome, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;

    let actor = super::verified_account_actor(&auth)?;
    let change = serde_json::from_str::<krausen_types::account::ProfileChange>(&change_json)
        .map_err(|_| ServerFnError::new("Invalid profile change"))?;
    let request_id = uuid::Uuid::parse_str(&request_id)
        .map_err(|_| ServerFnError::new("Invalid request identity"))?;
    Ok(krausen_app::account::project_save(
        krausen_app::account::save(&ctx.db, &actor, change, expected_revision, request_id).await,
    ))
}

// Helpers — delegate to shared extractors in parent module
#[cfg(feature = "ssr")]
use super::{extract_auth, extract_context};
