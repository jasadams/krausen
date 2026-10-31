// SPDX-License-Identifier: AGPL-3.0-or-later

//! Server functions for Security settings.
//!
//! These replace the REST API calls for password, TOTP, session, and passkey management:
//! - `POST /auth/set-password` -> `set_password()`
//! - `POST /auth/change-password` -> `change_password()`
//! - `GET  /auth/2fa/status` -> `get_totp_status()`
//! - `POST /auth/2fa/setup` -> `setup_totp()`
//! - `POST /auth/2fa/enable` -> `enable_totp()`
//! - `POST /auth/2fa/disable` -> `disable_totp()`
//! - `GET  /auth/sessions` -> `get_sessions()`
//! - `DELETE /auth/sessions/{id}` -> `revoke_session()`
//! - `POST /auth/logout-all` -> `logout_all_sessions()`
//! - `GET  /auth/passkeys/list` -> `list_passkeys()`
//! - `POST /auth/passkeys/add/start` -> `start_passkey_registration()`
//! - `POST /auth/passkeys/add/complete` -> `complete_passkey_registration()`
//! - `DELETE /auth/passkeys/{id}` -> `delete_passkey()`
//! - `PATCH /auth/passkeys/{id}` -> `rename_passkey()`
//!
//! Calls the same service-layer code as `apps/server/src/routes/auth_password.rs`,
//! `apps/server/src/routes/auth_totp.rs`, `apps/server/src/routes/auth_passkeys.rs`,
//! and `apps/server/src/routes/auth.rs`.

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ssr")]
use super::{IntoServerFnError, extract_auth, extract_context};

/// TOTP status returned by `get_totp_status()`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TotpStatus {
    pub enabled: bool,
}

/// Check whether the current user has a password set.
#[server(prefix = "/leptos-api")]
pub async fn has_password() -> Result<bool, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;

    let actor = super::verified_account_actor(&auth)?;
    let status = krausen_app::security::status(&ctx.db, &actor)
        .await
        .map_err(|_| ServerFnError::new("Unable to load security status"))?;
    Ok(status.has_password)
}

/// Load the current account's security state and observed revision.
#[server(prefix = "/leptos-api")]
pub async fn get_security_status() -> Result<krausen_types::account::SecurityStatus, ServerFnError>
{
    let auth = extract_auth().await?;
    let ctx = extract_context()?;
    let actor = super::verified_account_actor(&auth)?;
    krausen_app::security::status(&ctx.db, &actor)
        .await
        .map_err(|_| ServerFnError::new("Unable to load security status"))
}

/// Set a password for a user who does not yet have one (e.g. OAuth-only users).
///
/// Mirrors the validation in `apps/server/src/routes/auth_password.rs::set_password`:
/// - Password must be at least 8 characters.
/// - User must NOT already have a password.
#[server(prefix = "/leptos-api")]
pub async fn set_password(
    new_password: String,
    expected_revision: i64,
    request_id: String,
) -> Result<krausen_types::account::SetPasswordOutcome, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;
    let actor = super::verified_account_actor(&auth)?;
    let request_id = uuid::Uuid::parse_str(&request_id)
        .map_err(|_| ServerFnError::new("Invalid request identity"))?;
    Ok(krausen_app::security::project_set_password(
        krausen_app::security::set_password(
            &ctx.db,
            &actor,
            new_password,
            expected_revision,
            request_id,
            ctx.auth_state.jwt_secret.as_bytes(),
        )
        .await,
    ))
}

/// Change password for a user who already has one.
///
/// Mirrors the validation in `apps/server/src/routes/auth_password.rs::change_password`:
/// - New password must be at least 8 characters.
/// - Current password must be verified first.
#[server(prefix = "/leptos-api")]
pub async fn change_password(
    current_password: String,
    new_password: String,
    expected_revision: i64,
    request_id: String,
) -> Result<krausen_types::account::PasswordChangeOutcome, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;

    let actor = super::verified_account_actor(&auth)?;
    let request_id = uuid::Uuid::parse_str(&request_id)
        .map_err(|_| ServerFnError::new("Invalid request identity"))?;
    Ok(krausen_app::security::project_password_change(
        krausen_app::security::change_password(
            &ctx.db,
            &actor,
            current_password,
            new_password,
            expected_revision,
            request_id,
            ctx.auth_state.jwt_secret.as_bytes(),
        )
        .await,
    ))
}

// ---------------------------------------------------------------------------
// TOTP 2FA server functions
// ---------------------------------------------------------------------------

/// Check whether the current user has TOTP 2FA enabled.
///
/// Mirrors `GET /auth/2fa/status` in `apps/server/src/routes/auth_totp.rs`.
#[server(prefix = "/leptos-api")]
pub async fn get_totp_status() -> Result<TotpStatus, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;

    let actor = super::verified_account_actor(&auth)?;
    let status = krausen_app::security::status(&ctx.db, &actor)
        .await
        .map_err(|_| ServerFnError::new("Unable to load security status"))?;
    Ok(TotpStatus {
        enabled: status.totp_enabled,
    })
}

/// Begin 2FA setup: generate a secret and QR code data URI.
///
/// The secret is stored in Redis (10 min TTL) until the user confirms with
/// a verification code via `enable_totp()`.
///
/// Mirrors `POST /auth/2fa/setup` in `apps/server/src/routes/auth_totp.rs`.
#[server(prefix = "/leptos-api")]
pub async fn setup_totp(
    expected_revision: i64,
) -> Result<krausen_types::account::TotpSetupOutcome, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;
    let actor = super::verified_account_actor(&auth)?;
    let kv = ctx.kv()?;
    Ok(krausen_app::security::project_totp_setup(
        krausen_app::security::setup_totp(&ctx.db, &kv, &actor, expected_revision).await,
    ))
}

/// Confirm 2FA setup by verifying a TOTP code against the pending secret.
///
/// On success the secret is persisted in `user_auth_methods`. On failure the
/// pending secret is re-stored in Redis so the user can retry.
///
/// Mirrors `POST /auth/2fa/enable` in `apps/server/src/routes/auth_totp.rs`.
#[server(prefix = "/leptos-api")]
pub async fn enable_totp(
    code: String,
    expected_revision: i64,
    request_id: String,
) -> Result<krausen_types::account::TotpEnableOutcome, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;
    let actor = super::verified_account_actor(&auth)?;
    let kv = ctx.kv()?;
    let request_id = uuid::Uuid::parse_str(&request_id)
        .map_err(|_| ServerFnError::new("Invalid request identity"))?;
    Ok(krausen_app::security::project_totp_enable(
        krausen_app::security::enable_totp(
            &ctx.db,
            &kv,
            &actor,
            &code,
            expected_revision,
            request_id,
            ctx.auth_state.jwt_secret.as_bytes(),
        )
        .await,
    ))
}

/// Disable 2FA for the authenticated user.
///
/// Mirrors `POST /auth/2fa/disable` in `apps/server/src/routes/auth_totp.rs`.
/// The REST handler does not require a TOTP code — it simply removes the auth
/// method for the already-authenticated user.
#[server(prefix = "/leptos-api")]
pub async fn disable_totp(
    expected_revision: i64,
    request_id: String,
) -> Result<krausen_types::account::TotpDisableOutcome, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;
    let actor = super::verified_account_actor(&auth)?;
    let request_id = uuid::Uuid::parse_str(&request_id)
        .map_err(|_| ServerFnError::new("Invalid request identity"))?;
    Ok(krausen_app::security::project_totp_disable(
        krausen_app::security::disable_totp(&ctx.db, &actor, expected_revision, request_id).await,
    ))
}

// ---------------------------------------------------------------------------
// Session management server functions
// ---------------------------------------------------------------------------

/// A single session entry returned by `get_sessions()`.
///
/// Maps to the `SessionInfo` fields from `krausen_auth::token_service` plus
/// the `is_current` flag computed by comparing the caller's refresh token
/// cookie family against each session's family.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionEntry {
    pub token_id: String,
    pub revision: i64,
    pub kind: String,
    pub created_at: String,
    pub last_used: Option<String>,
    pub expires_at: String,
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
    pub country_code: Option<String>,
    pub oauth_client_name: Option<String>,
    pub is_current: bool,
}

/// A bounded session inventory that tells the UI when more rows exist.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionList {
    pub sessions: Vec<SessionEntry>,
    pub truncated: bool,
}

/// Get all active sessions for the current user.
///
/// Mirrors `GET /auth/sessions` in `apps/server/src/routes/auth.rs`.
/// Determines the current session by comparing the refresh token cookie's
/// family_id against each session's family_id.
#[server(prefix = "/leptos-api")]
pub async fn get_sessions() -> Result<SessionList, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;

    let actor = super::verified_account_actor(&auth)?;
    let page = krausen_app::security::sessions(&ctx.db, &actor)
        .await
        .map_err(|_| ServerFnError::new("Unable to load sessions"))?;
    Ok(SessionList {
        sessions: page
            .items
            .into_iter()
            .map(|s| SessionEntry {
                token_id: s.credential_id,
                revision: s.revision,
                kind: s.kind,
                created_at: s.created_at.unwrap_or_default(),
                last_used: s.last_used,
                expires_at: chrono::DateTime::from_timestamp(s.expires_at, 0)
                    .map(|v| v.to_rfc3339())
                    .unwrap_or_default(),
                user_agent: s.user_agent,
                ip_address: s.ip_address,
                country_code: s.country_code,
                oauth_client_name: s.client_name,
                is_current: s.is_current,
            })
            .collect(),
        truncated: page.truncated,
    })
}

/// Revoke a specific session by token ID.
///
/// Mirrors `DELETE /auth/sessions/{token_id}` in `apps/server/src/routes/auth.rs`.
/// Revokes the entire token family so rotated tokens in the same session are
/// also invalidated.
#[server(prefix = "/leptos-api")]
pub async fn revoke_session(
    token_id: String,
    expected_revision: i64,
    request_id: String,
) -> Result<krausen_types::account::TeamChangeOutcome, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;
    let actor = super::verified_account_actor(&auth)?;
    let request_id = uuid::Uuid::parse_str(&request_id)
        .map_err(|_| ServerFnError::new("Invalid request identity"))?;
    Ok(krausen_app::team::project_change(
        krausen_app::security::revoke(&ctx.db, &actor, &token_id, expected_revision, request_id)
            .await,
    ))
}

/// Log out the current session.
///
/// Mirrors `POST /auth/logout` in `apps/server/src/routes/auth.rs`:
/// 1. Reads the refresh token cookie.
/// 2. Revokes the entire token family so rotated tokens are also invalidated.
/// 3. Clears both auth cookies (access_token + refresh_token) via `ResponseOptions`.
///
/// Does NOT require `extract_auth()` — the token may already be invalid
/// (e.g. if the access token expired) but we still want to clear cookies.
#[server(prefix = "/leptos-api")]
pub async fn logout() -> Result<(), ServerFnError> {
    let ctx = extract_context()?;

    let headers: axum::http::HeaderMap = leptos_axum::extract()
        .await
        .map_err(|e| ServerFnError::new(format!("Failed to extract headers: {e}")))?;

    let refresh_token_name = &krausen_core::constants::get().cookies.refresh_token_name;
    let raw_token = krausen_auth::cookies::get_cookie_value(&headers, refresh_token_name);

    krausen_auth::security_service::logout(&ctx.db, raw_token)
        .await
        .into_sfn()?;

    // Clear both HTTPOnly cookies so the browser forgets the session.
    let response_options = leptos::prelude::expect_context::<leptos_axum::ResponseOptions>();
    let mut cookie_headers = axum::http::HeaderMap::new();
    krausen_auth::cookies::clear_token_cookies(&mut cookie_headers);
    for (name, value) in cookie_headers.iter() {
        if name == axum::http::header::SET_COOKIE {
            response_options.append_header(name.clone(), value.clone());
        }
    }

    Ok(())
}

/// Log out from all devices by revoking every refresh token for the user.
///
/// Mirrors `POST /auth/logout-all` in `apps/server/src/routes/auth.rs`.
#[server(prefix = "/leptos-api")]
pub async fn logout_all_sessions(
    request_id: String,
) -> Result<krausen_types::account::RevokeAllOutcome, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;
    let actor = super::verified_account_actor(&auth)?;
    let request_id = uuid::Uuid::parse_str(&request_id)
        .map_err(|_| ServerFnError::new("Invalid request identity"))?;
    let outcome = krausen_app::security::project_revoke_all(
        krausen_app::security::revoke_all(&ctx.db, &actor, request_id).await,
    );
    if matches!(
        outcome,
        krausen_types::account::RevokeAllOutcome::Revoked { .. }
    ) {
        let response = leptos::prelude::expect_context::<leptos_axum::ResponseOptions>();
        let mut headers = axum::http::HeaderMap::new();
        krausen_auth::cookies::clear_token_cookies(&mut headers);
        for value in headers.get_all(axum::http::header::SET_COOKIE) {
            response.append_header(axum::http::header::SET_COOKIE, value.clone());
        }
    }
    Ok(outcome)
}

// ---------------------------------------------------------------------------
// Passkey management server functions
// ---------------------------------------------------------------------------

/// A single passkey entry returned by `list_passkeys()`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PasskeyInfo {
    pub credential_id: String,
    pub name: String,
    pub created_at: Option<String>,
    pub last_used: Option<String>,
}

/// List all passkeys for the authenticated user.
///
/// Mirrors `GET /auth/passkeys/list` in `apps/server/src/routes/auth_passkeys.rs`.
#[server(prefix = "/leptos-api")]
pub async fn list_passkeys() -> Result<Vec<PasskeyInfo>, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;

    let actor = super::verified_account_actor(&auth)?;
    let passkeys = krausen_app::security::passkeys(&ctx.db, &actor)
        .await
        .map_err(|_| ServerFnError::new("Unable to load passkeys"))?;
    Ok(passkeys
        .into_iter()
        .map(|passkey| PasskeyInfo {
            credential_id: passkey.credential_id,
            name: passkey.name,
            created_at: passkey.created_at,
            last_used: passkey.last_used,
        })
        .collect())
}

/// Start passkey registration for the authenticated user.
///
/// Returns a JSON string containing the challenge_id and WebAuthn options
/// that the browser needs for `navigator.credentials.create()`.
///
/// Mirrors `POST /auth/passkeys/add/start` in `apps/server/src/routes/auth_passkeys.rs`.
#[server(prefix = "/leptos-api")]
pub async fn start_passkey_registration(
    device_name: String,
    expected_revision: i64,
) -> Result<krausen_types::account::PasskeyRegistrationStartOutcome, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;
    let actor = super::verified_account_actor(&auth)?;
    let webauthn = ctx.webauthn()?;
    let kv = ctx.kv()?;
    Ok(krausen_app::security::project_passkey_start(
        krausen_app::security::start_passkey_registration(
            &ctx.db,
            &kv,
            webauthn,
            &actor,
            &device_name,
            expected_revision,
        )
        .await,
    ))
}

/// Complete passkey registration by verifying the browser credential.
///
/// Receives the challenge_id and the PublicKeyCredential JSON from the browser.
///
/// Mirrors `POST /auth/passkeys/add/complete` in `apps/server/src/routes/auth_passkeys.rs`.
#[server(prefix = "/leptos-api")]
pub async fn complete_passkey_registration(
    credential_json: String,
    expected_revision: i64,
    request_id: String,
) -> Result<krausen_types::account::PasskeyRegistrationCompleteOutcome, ServerFnError> {
    use webauthn_rs::prelude::RegisterPublicKeyCredential;

    let auth = extract_auth().await?;
    let ctx = extract_context()?;
    let actor = super::verified_account_actor(&auth)?;
    let webauthn = ctx.webauthn()?;
    let kv = ctx.kv()?;
    let request_id = uuid::Uuid::parse_str(&request_id)
        .map_err(|_| ServerFnError::new("Invalid request identity"))?;

    let data: serde_json::Value = serde_json::from_str(&credential_json)
        .map_err(|e| ServerFnError::new(format!("Invalid credential JSON: {e}")))?;

    let challenge_id = data["challenge_id"]
        .as_str()
        .ok_or_else(|| ServerFnError::new("Missing challenge_id"))?;

    let credential: RegisterPublicKeyCredential =
        serde_json::from_value(data["credential"].clone())
            .map_err(|e| ServerFnError::new(format!("Invalid credential: {e}")))?;

    Ok(krausen_app::security::project_passkey_complete(
        krausen_app::security::complete_passkey_registration(
            &ctx.db,
            &kv,
            webauthn,
            &actor,
            challenge_id,
            &credential,
            expected_revision,
            request_id,
        )
        .await,
    ))
}

/// Delete a passkey for the authenticated user.
///
/// Mirrors `DELETE /auth/passkeys/{credential_id}` in `apps/server/src/routes/auth_passkeys.rs`.
#[server(prefix = "/leptos-api")]
pub async fn delete_passkey(
    credential_id: String,
    expected_revision: i64,
    request_id: String,
) -> Result<krausen_types::account::PasskeyDeleteOutcome, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;
    let actor = super::verified_account_actor(&auth)?;
    let request_id = uuid::Uuid::parse_str(&request_id)
        .map_err(|_| ServerFnError::new("Invalid request identity"))?;
    Ok(krausen_app::security::project_passkey_delete(
        krausen_app::security::delete_passkey(
            &ctx.db,
            &actor,
            &credential_id,
            expected_revision,
            request_id,
        )
        .await,
    ))
}

/// Rename a passkey for the authenticated user.
///
/// Mirrors `PATCH /auth/passkeys/{credential_id}` in `apps/server/src/routes/auth_passkeys.rs`.
#[server(prefix = "/leptos-api")]
pub async fn rename_passkey(
    credential_id: String,
    name: String,
    expected_revision: i64,
    request_id: String,
) -> Result<krausen_types::account::PasskeyRenameOutcome, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;
    let actor = super::verified_account_actor(&auth)?;
    let request_id = uuid::Uuid::parse_str(&request_id)
        .map_err(|_| ServerFnError::new("Invalid request identity"))?;
    Ok(krausen_app::security::project_passkey_rename(
        krausen_app::security::rename_passkey(
            &ctx.db,
            &actor,
            &credential_id,
            &name,
            expected_revision,
            request_id,
        )
        .await,
    ))
}
